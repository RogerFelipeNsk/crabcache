//! TCP server: thread-per-core I/O, per-connection request/reply loop, background maintenance.
//!
//! An acceptor hands each new connection to one of N I/O threads. Every I/O thread runs its own
//! single-threaded tokio runtime with its own kqueue/epoll instance, so a connection is read, executed
//! and written on one thread with no cross-thread wakeups. The keyspace shards are shared by all
//! threads.
//!
//! Placement is adaptive: only `ceil(connections / io_conns_per_thread)` threads are used, and a new
//! connection goes to the least loaded of them. Spreading a light load over every thread makes each
//! one wake up per request; measured with redis-benchmark (50 clients, no pipelining) that cost 40%
//! of throughput going from 1 to 10 threads.
//!
//! Each read is followed by parsing and executing every complete command in the buffer, appending
//! replies to one output buffer that is written with a single syscall. With pipelining this batches
//! many replies per write.

use crate::commands::{self, Session, Shared, Stats};
use crate::config::Config;
use crate::protocol::{Args, Parsed, Parser, reply};
use crate::store::PackCursor;
use crate::store::compress::TrainOutcome;
use bytes::{Buf, BytesMut};
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

const READ_CHUNK: usize = 16 * 1024;
/// Flush replies mid-batch once this much output is pending, so a pipelined batch of large GETs does
/// not build an unbounded output buffer.
const OUT_FLUSH: usize = 64 * 1024;
/// Buffers that grew past this are released after use instead of being kept per idle connection.
const SHRINK_ABOVE: usize = 256 * 1024;

type Handoff = (std::net::TcpStream, SocketAddr);

/// The acceptor's handle to one I/O thread.
struct Worker {
    tx: mpsc::UnboundedSender<Handoff>,
    /// Connections currently served by the thread.
    conns: Arc<AtomicUsize>,
}

/// Picks the I/O thread for a new connection: the least loaded among the first
/// `ceil(total / per_thread)` threads (all threads when `per_thread` is 0).
fn pick_worker(loads: &[usize], total: usize, per_thread: usize) -> usize {
    let active = if per_thread == 0 {
        loads.len()
    } else {
        total.div_ceil(per_thread).clamp(1, loads.len())
    };
    (0..active).min_by_key(|&i| loads[i]).unwrap_or(0)
}

pub struct Server {
    listener: TcpListener,
    shared: Arc<Shared>,
}

impl Server {
    pub async fn bind(config: Config) -> io::Result<Self> {
        let listener = TcpListener::bind((config.bind.as_str(), config.port)).await?;
        Ok(Self {
            listener,
            shared: Arc::new(Shared::new(config)),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn shared(&self) -> Arc<Shared> {
        Arc::clone(&self.shared)
    }

    /// Starts the I/O threads and serves until `shutdown` resolves. The caller's runtime only runs the
    /// acceptor and the background maintenance tasks.
    pub async fn run(self, shutdown: impl Future<Output = ()>) {
        let (stop_tx, stop_rx) = watch::channel(false);
        let mut workers = Vec::new();
        let mut threads = Vec::new();
        for i in 0..self.shared.config.worker_threads() {
            let (tx, rx) = mpsc::unbounded_channel::<Handoff>();
            let conns = Arc::new(AtomicUsize::new(0));
            let (shared, stop, load) = (self.shared(), stop_rx.clone(), Arc::clone(&conns));
            let thread = std::thread::Builder::new()
                .name(format!("crabcache-io-{i}"))
                .spawn(move || io_thread(rx, shared, load, stop))
                .expect("spawn I/O thread");
            workers.push(Worker { tx, conns });
            threads.push(thread);
        }
        let per_thread = self.shared.config.io_conns_per_thread;

        let expire = tokio::spawn(expire_task(self.shared()));
        let evict = tokio::spawn(evict_task(self.shared()));
        let pack = tokio::spawn(pack_task(self.shared()));
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                res = self.listener.accept() => match res {
                    Ok((stream, addr)) => {
                        if let Some(stream) = self.admit(stream).await {
                            let loads: Vec<usize> = workers.iter().map(|w| w.conns.load(Relaxed)).collect();
                            let total = self.shared.stats.connected_clients.load(Relaxed) as usize;
                            let w = &workers[pick_worker(&loads, total, per_thread)];
                            w.conns.fetch_add(1, Relaxed);
                            if w.tx.send((stream, addr)).is_err() {
                                w.conns.fetch_sub(1, Relaxed);
                                self.shared.stats.connected_clients.fetch_sub(1, Relaxed);
                            }
                        }
                    }
                    Err(e) => {
                        // Typically EMFILE: back off instead of dying.
                        warn!(error = %e, "accept failed");
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        }
        expire.abort();
        evict.abort();
        pack.abort();
        let _ = stop_tx.send(true);
        drop(workers);
        let _ = tokio::task::spawn_blocking(move || {
            for t in threads {
                let _ = t.join();
            }
        })
        .await;
    }

    /// Applies `maxclients` and detaches the socket from the acceptor's reactor for hand-off.
    async fn admit(&self, mut stream: TcpStream) -> Option<std::net::TcpStream> {
        let stats = &self.shared.stats;
        stats.connections_received.fetch_add(1, Relaxed);
        if stats.connected_clients.load(Relaxed) as usize >= self.shared.config.maxclients {
            stats.rejected_connections.fetch_add(1, Relaxed);
            let _ = stream
                .write_all(b"-ERR max number of clients reached\r\n")
                .await;
            return None;
        }
        match stream.into_std() {
            Ok(s) => {
                stats.connected_clients.fetch_add(1, Relaxed);
                Some(s)
            }
            Err(e) => {
                warn!(error = %e, "could not detach accepted socket");
                None
            }
        }
    }
}

/// Releases a connection's slot in the global and per-thread counts when it ends, including when its
/// task is cancelled at shutdown.
struct ClientSlot<'a>(&'a Stats, &'a AtomicUsize);

impl Drop for ClientSlot<'_> {
    fn drop(&mut self) {
        self.0.connected_clients.fetch_sub(1, Relaxed);
        self.1.fetch_sub(1, Relaxed);
    }
}

/// One I/O thread: a single-threaded runtime serving the connections handed to it.
fn io_thread(
    mut rx: mpsc::UnboundedReceiver<Handoff>,
    shared: Arc<Shared>,
    conns: Arc<AtomicUsize>,
    mut stop: watch::Receiver<bool>,
) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            warn!(error = %e, "failed to start I/O thread runtime");
            return;
        }
    };
    rt.block_on(async move {
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                msg = rx.recv() => {
                    let Some((std_stream, addr)) = msg else { break };
                    let (shared, conns) = (Arc::clone(&shared), Arc::clone(&conns));
                    tokio::spawn(async move {
                        let _slot = ClientSlot(&shared.stats, &conns);
                        let stream = match TcpStream::from_std(std_stream) {
                            Ok(s) => s,
                            Err(e) => {
                                warn!(error = %e, "could not register connection");
                                return;
                            }
                        };
                        if let Err(e) = serve(stream, &shared, addr).await {
                            debug!(%addr, error = %e, "connection closed with error");
                        }
                    });
                }
            }
        }
    });
    // Dropping the runtime cancels the remaining connection tasks.
}

async fn serve(mut stream: TcpStream, shared: &Shared, addr: SocketAddr) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let limits = shared.config.limits();
    let query_limit = shared.config.client_query_buffer_limit as usize;
    let mut session = Session::new(shared, addr.to_string());
    let mut parser = Parser::new();
    let mut buf = BytesMut::with_capacity(READ_CHUNK);
    let mut out: Vec<u8> = Vec::with_capacity(READ_CHUNK);

    loop {
        // Reserve for the pending bulk argument in one step (bounded by the query buffer limit).
        let want = parser
            .needed()
            .saturating_sub(buf.len())
            .clamp(READ_CHUNK, query_limit.max(READ_CHUNK));
        buf.reserve(want);
        if stream.read_buf(&mut buf).await? == 0 {
            return Ok(());
        }

        let clock = shared.db.clock();
        let mut pos = 0;
        let mut executed = 0u64;
        loop {
            match parser.parse(&buf[pos..], &limits) {
                Ok(Parsed::Command { consumed, inline }) => {
                    {
                        let base: &[u8] = if inline { &parser.scratch } else { &buf[pos..] };
                        let args = Args::new(base, &parser.args);
                        commands::execute(shared, &mut session, &args, clock, &mut out);
                    }
                    pos += consumed;
                    executed += 1;
                    if session.close {
                        break;
                    }
                    if out.len() >= OUT_FLUSH {
                        stream.write_all(&out).await?;
                        out.clear();
                    }
                }
                Ok(Parsed::Empty { consumed }) => pos += consumed,
                Ok(Parsed::Incomplete) => break,
                Err(e) => {
                    reply::error(&mut out, &format!("ERR Protocol error: {}", e.0));
                    stream.write_all(&out).await?;
                    debug!(%addr, error = %e.0, "protocol error");
                    return Ok(());
                }
            }
        }
        buf.advance(pos);
        shared.stats.commands_processed.fetch_add(executed, Relaxed);

        if !out.is_empty() {
            stream.write_all(&out).await?;
            out.clear();
        }
        if session.close {
            return Ok(());
        }
        if buf.len() > query_limit {
            warn!(%addr, buffered = buf.len(), "closing client over query buffer limit");
            return Ok(());
        }
        if out.capacity() > SHRINK_ABOVE {
            out = Vec::with_capacity(READ_CHUNK);
        }
        if buf.is_empty() && buf.capacity() > SHRINK_ABOVE {
            buf = BytesMut::with_capacity(READ_CHUNK);
        }
    }
}

/// Active expiration: every 100 ms, delete keys whose deadline has passed (bounded work per shard).
async fn expire_task(shared: Arc<Shared>) {
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tick.tick().await;
        while shared.db.expire_cycle(256) {
            tokio::task::yield_now().await;
        }
    }
}

/// Background eviction when writers could not stay under `maxmemory` by evicting from their own shard.
async fn evict_task(shared: Arc<Shared>) {
    let mut cursor = 0;
    loop {
        tokio::select! {
            _ = shared.db.evict_notify.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
        while shared.db.evict_cycle(&mut cursor, 1024) == 1024 {
            tokio::task::yield_now().await;
        }
    }
}

/// CrabPack: samples values, trains dictionaries on a blocking thread, and compresses idle values.
/// Bounded work per tick keeps the acceptor responsive.
async fn pack_task(shared: Arc<Shared>) {
    const SAMPLES_PER_TICK: usize = 64;
    const SCAN_PER_STEP: usize = 256;
    const STEPS_PER_TICK: usize = 64;
    let mut cursor = PackCursor::default();
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    loop {
        tick.tick().await;
        if !shared.db.codec.enabled() {
            continue;
        }
        for (prefix, samples) in shared.db.pack_sample(SAMPLES_PER_TICK) {
            let shared = Arc::clone(&shared);
            tokio::task::spawn_blocking(move || {
                let label = String::from_utf8_lossy(&prefix).into_owned();
                let secs = shared.db.clock().secs;
                match shared.db.codec.train(prefix, samples, secs) {
                    TrainOutcome::Accepted { id, ratio } => {
                        info!(prefix = %label, id, ratio = format!("{ratio:.2}"), "crabpack: dictionary trained")
                    }
                    TrainOutcome::Rejected { ratio } => {
                        info!(prefix = %label, ratio = format!("{ratio:.2}"), "crabpack: dictionary rejected (too little gain)")
                    }
                    TrainOutcome::Failed => {
                        warn!(prefix = %label, "crabpack: dictionary training failed")
                    }
                }
            });
        }
        for _ in 0..STEPS_PER_TICK {
            shared.db.pack_step(&mut cursor, SCAN_PER_STEP);
            tokio::task::yield_now().await;
        }
    }
}

/// Logs the listening address and warns about unauthenticated public binds.
pub fn log_startup(config: &Config, addr: SocketAddr) {
    info!(
        version = crate::VERSION,
        %addr,
        threads = config.worker_threads(),
        shards = config.shard_count(),
        maxmemory = config.maxmemory,
        policy = config.maxmemory_policy.name(),
        compression = config.compression,
        compression_min_idle = config.compression_min_idle,
        compression_min_size = config.compression_min_size,
        "crabcache ready"
    );
    if !addr.ip().is_loopback() && config.requirepass.is_none() {
        warn!(
            "listening on a non-loopback address without --requirepass; anyone who can reach this port can read and write data"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::pick_worker;

    #[test]
    fn packs_connections_before_spreading() {
        // 10 threads, 32 per thread: the first 32 connections share thread 0.
        assert_eq!(pick_worker(&[0; 10], 1, 32), 0);
        assert_eq!(pick_worker(&[31, 0, 0, 0, 0, 0, 0, 0, 0, 0], 32, 32), 0);
        // The 33rd brings a second thread into use and goes to the emptier one.
        assert_eq!(pick_worker(&[32, 0, 0, 0, 0, 0, 0, 0, 0, 0], 33, 32), 1);
        // Never beyond the available threads, always the least loaded active one.
        assert_eq!(pick_worker(&[5, 3, 4], 10_000, 32), 1);
        // 0 = spread over every thread.
        assert_eq!(pick_worker(&[1, 1, 0, 1], 3, 0), 2);
    }
}
