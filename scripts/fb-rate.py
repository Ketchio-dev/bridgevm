#!/usr/bin/env python3
import sys
import time
import struct

MAGIC = 0x42564642

def read_seq(path):
    try:
        with open(path, 'rb') as f:
            data = f.read(64)
        if len(data) < 32:
            return None
        magic = struct.unpack_from('<I', data, 0)[0]
        if magic != MAGIC:
            return None
        seq = struct.unpack_from('<Q', data, 24)[0]
        return seq
    except (OSError, IOError, struct.error):
        return None

def main():
    if len(sys.argv) < 2:
        sys.exit(1)
    path = sys.argv[1]
    duration = float(sys.argv[2]) if len(sys.argv) > 2 else 30.0

    wait_deadline = time.monotonic() + 20.0
    seq = None
    while time.monotonic() < wait_deadline:
        seq = read_seq(path)
        if seq is not None:
            break
        time.sleep(0.05)
    if seq is None:
        sys.exit(2)

    t0 = time.monotonic()
    last_seq = seq
    total_frames = 0
    interval_frames = 0
    rates_1s = []
    next_t = 1.0
    report_n = 0

    while True:
        now = time.monotonic()
        elapsed = now - t0

        seq = read_seq(path)
        if seq is not None:
            if seq < last_seq:
                last_seq = seq
            else:
                frames = (seq // 2) - (last_seq // 2)
                total_frames += frames
                interval_frames += frames
                last_seq = seq

        while next_t <= duration and elapsed >= next_t:
            report_n += 1
            rate = float(interval_frames)
            rates_1s.append(rate)
            avg = total_frames / next_t if next_t > 0 else 0.0
            print('t=%ds rate_1s=%.1f avg=%.1f' % (report_n, rate, avg))
            sys.stdout.flush()
            interval_frames = 0
            next_t += 1.0

        if elapsed >= duration:
            break

        sleep_for = min(0.05, t0 + duration - time.monotonic())
        if sleep_for > 0:
            time.sleep(sleep_for)

    now = time.monotonic()
    elapsed = now - t0
    seq = read_seq(path)
    if seq is not None:
        if seq < last_seq:
            last_seq = seq
        else:
            frames = (seq // 2) - (last_seq // 2)
            total_frames += frames
            interval_frames += frames
            last_seq = seq

    while next_t <= duration and elapsed >= next_t:
        report_n += 1
        rate = float(interval_frames)
        rates_1s.append(rate)
        avg = total_frames / next_t if next_t > 0 else 0.0
        print('t=%ds rate_1s=%.1f avg=%.1f' % (report_n, rate, avg))
        sys.stdout.flush()
        interval_frames = 0
        next_t += 1.0

    seconds = time.monotonic() - t0
    avg_fps = total_frames / seconds if seconds > 0 else 0.0
    if rates_1s:
        min_1s = min(rates_1s)
        max_1s = max(rates_1s)
    else:
        min_1s = 0.0
        max_1s = 0.0

    print('RESULT frames=%d seconds=%.1f avg_fps=%.2f min_1s=%.1f max_1s=%.1f' % (
        total_frames, seconds, avg_fps, min_1s, max_1s))
    sys.exit(0)

if __name__ == '__main__':
    main()