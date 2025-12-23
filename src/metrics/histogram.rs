use std::collections::VecDeque;

/// Simple histogram for tracking latency metrics
pub struct LatencyHistogram {
    samples: VecDeque<f64>,
    max_samples: usize,
}

impl LatencyHistogram {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            max_samples: 10000, // Keep last 10k samples
        }
    }

    pub fn record(&mut self, latency_ms: f64) {
        self.samples.push_back(latency_ms);
        
        // Keep only the most recent samples
        if self.samples.len() > self.max_samples {
            self.samples.pop_front();
        }
    }

    pub fn get_samples(&self) -> Vec<f64> {
        self.samples.iter().cloned().collect()
    }

    pub fn percentile(&self, p: f64) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }

        let mut sorted: Vec<f64> = self.samples.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let index = ((p / 100.0) * (sorted.len() - 1) as f64) as usize;
        sorted[index.min(sorted.len() - 1)]
    }

    pub fn mean(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        
        self.samples.iter().sum::<f64>() / self.samples.len() as f64
    }

    pub fn max(&self) -> f64 {
        self.samples.iter().cloned().fold(0.0, f64::max)
    }

    pub fn count(&self) -> usize {
        self.samples.len()
    }
}