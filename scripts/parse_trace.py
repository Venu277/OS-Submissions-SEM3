#!/usr/bin/env python3
"""
scripts/parse_trace.py
Parses MicroOS machine-parseable serial trace logs into tabular/CSV format
and outputs event distribution statistics.

Specification: CA-6 Section 9 (Layer 2) & Section 23 (Trace Analysis)
"""

import sys
import os
import re
import csv
import argparse

TRACE_PATTERN = re.compile(
    r"\[TRACE\]\s+tick=(?P<tick>\d+)\s+event=(?P<event>\w+)(?:\s+(?P<fields>.*))?$"
)
KV_PATTERN = re.compile(r"(?P<key>\w+)=(?P<val>[^\s]+)")
LOG_PATTERN = re.compile(
    r"\[(?P<level>INFO |WARN |ERROR|DEBUG)\s+tick=(?P<tick>\d+)\]\s+(?P<msg>.*)$"
)

def parse_trace_file(filepath):
    events = []
    logs = []
    
    if not os.path.isfile(filepath):
        print(f"Error: Trace file not found: {filepath}")
        return events, logs

    with open(filepath, "r", encoding="utf-8", errors="replace") as f:
        for line_no, raw_line in enumerate(f, start=1):
            line = raw_line.strip()
            if not line:
                continue

            # Check for [TRACE] event
            trace_match = TRACE_PATTERN.search(line)
            if trace_match:
                tick = int(trace_match.group("tick"))
                event_name = trace_match.group("event")
                fields_str = trace_match.group("fields") or ""
                
                row = {
                    "line": line_no,
                    "tick": tick,
                    "event": event_name,
                }
                for kv in KV_PATTERN.finditer(fields_str):
                    row[kv.group("key")] = kv.group("val")
                events.append(row)
                continue

            # Check for [LEVEL tick=N] diagnostic log
            log_match = LOG_PATTERN.search(line)
            if log_match:
                logs.append({
                    "line": line_no,
                    "level": log_match.group("level").strip(),
                    "tick": int(log_match.group("tick")),
                    "message": log_match.group("msg"),
                })

    return events, logs

def print_summary(events, logs, filepath):
    print("=" * 60)
    print("      MicroOS Trace & Diagnostic Analysis Report")
    print("=" * 60)
    print(f"Source Trace File:   {filepath}")
    print(f"Total Trace Events:  {len(events)}")
    print(f"Total Console Logs:  {len(logs)}")
    print("-" * 60)

    if events:
        event_counts = {}
        for ev in events:
            ev_type = ev.get("event", "UNKNOWN")
            event_counts[ev_type] = event_counts.get(ev_type, 0) + 1

        print("Trace Event Distribution:")
        for ev_type, count in sorted(event_counts.items(), key=lambda x: -x[1]):
            print(f"  - {ev_type:<20}: {count:>5}")

        ticks = [ev["tick"] for ev in events]
        print(f"\nTick Range: min={min(ticks)}, max={max(ticks)}, delta={max(ticks)-min(ticks)}")
    else:
        print("No machine trace events found.")

    if logs:
        log_counts = {}
        for l in logs:
            lvl = l["level"]
            log_counts[lvl] = log_counts.get(lvl, 0) + 1
        print("\nDiagnostic Logs by Level:")
        for lvl, count in sorted(log_counts.items()):
            print(f"  - [{lvl}]: {count:>5}")

    print("=" * 60)

def export_csv(events, out_path):
    if not events:
        return
    # Collect all unique field keys
    fieldnames = ["line", "tick", "event"]
    extra_keys = sorted({k for row in events for k in row.keys() if k not in fieldnames})
    fieldnames.extend(extra_keys)

    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in events:
            writer.writerow(row)
    print(f"[EXPORT] Trace CSV saved to: {out_path}")

def main():
    parser = argparse.ArgumentParser(description="MicroOS Trace Parser (CA-6 Layer 2)")
    parser.add_argument("--trace", "-t", default="traces/step_a2_diagnostics.log", help="Path to trace log")
    parser.add_argument("--csv", "-c", default="results/trace_events.csv", help="Path to export CSV")
    args = parser.parse_args()

    events, logs = parse_trace_file(args.trace)
    print_summary(events, logs, args.trace)

    if events:
        export_csv(events, args.csv)

if __name__ == "__main__":
    main()
