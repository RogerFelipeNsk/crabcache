"""Regression checks for false benchmark successes and measurement parsing."""
import copy
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import benchmark as b


class BenchmarkValidation(unittest.TestCase):
    def test_get_count_difference_is_bounded_by_inflight_pipeline(self):
        self.assertEqual(b.validate_get_counts(1100, 1000, 48, 16), 100)
        self.assertEqual(b.validate_get_counts(1000, 1000, 48, 1), 0)
        for server in [999, 1769]:
            with self.assertRaises(RuntimeError):
                b.validate_get_counts(server, 1000, 48, 16)

    def test_resp_byte_count_handles_utf8_newlines_and_binary(self):
        values = ['João\r\nAraújo'.encode(), b'\0\xff\r\n', b'']
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'input.resp'
            path.write_bytes(b''.join(b.encode(['SET', f'key:{i}', v]) for i, v in enumerate(values)))
            info, samples = b.dataset_info(path, 3)
            self.assertEqual(info['value_bytes'], sum(map(len, values)))
            self.assertEqual([value for _, value in samples], values)
            with self.assertRaises(RuntimeError):
                b.dataset_info(path, 4)

    def test_truncated_or_error_replies_are_rejected(self):
        for wire in [b'$4\r\nabc', b'$3\r\nabcXX', b'*1\r\n', b'-OOM error\r\n']:
            with self.subTest(wire=wire), self.assertRaises(RuntimeError):
                b.read_resp(io.BytesIO(wire))

    def test_memtier_requires_hits_and_no_errors(self):
        row = {'Ops/sec': 100, 'Hits/sec': 90, 'Misses/sec': 0,
               'Connection Errors': 0, 'Percentile Latencies': {'p50.00': .1, 'p99.00': .2}}
        data = {'ALL STATS': {name: copy.deepcopy(row) for name in ['Sets', 'Gets', 'Totals']}}
        self.assertEqual(b.parse_memtier(data)['p99_ms'], .2)
        for key, value in [('Hits/sec', 0), ('Misses/sec', 1), ('Connection Errors', 1)]:
            changed = copy.deepcopy(data)
            changed['ALL STATS']['Gets'][key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                b.parse_memtier(changed)

    def test_missing_or_invalid_csv_results_are_rejected(self):
        header = '"test","rps","p50_latency_ms","p99_latency_ms"\n'
        rows = '"SET","100","0.1","0.2"\n"GET","200","0.3","0.4"\n'
        self.assertEqual(b.parse_redis_benchmark(header + rows)['GET']['ops_s'], 200)
        for text in [header, header + rows.splitlines()[0], (header + rows).replace('"100"', '"nan"'),
                     (header + rows).replace('"0.1"', '"nan"')]:
            with self.assertRaises(RuntimeError):
                b.parse_redis_benchmark(text)

    def test_compression_timeout_rejects_partial_coverage(self):
        bench = b.Bench.__new__(b.Bench)
        with patch.object(bench, 'info', return_value={'compressed_keys': '99'}), \
                patch.dict('os.environ', {'PACK_TIMEOUT': '0'}):
            with self.assertRaisesRegex(RuntimeError, '99/100'):
                bench.pack(None, 100, 0)

    def test_load_checks_errors_reply_count_and_dbsize(self):
        from unittest.mock import Mock
        bench = b.Bench.__new__(b.Bench)
        client = Mock()
        client.command.return_value = 10
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'load.resp'
            path.touch()
            for output in ['errors: 1, replies: 10', 'errors: 0, replies: 9', '']:
                with patch.object(bench, 'run', return_value=output), self.assertRaises(RuntimeError):
                    bench.load(client, 12345, path, 10, [])
            with patch.object(bench, 'run', return_value='errors: 0, replies: 10'):
                client.command.return_value = 9
                with self.assertRaises(RuntimeError):
                    bench.load(client, 12345, path, 10, [])


if __name__ == '__main__':
    unittest.main()
