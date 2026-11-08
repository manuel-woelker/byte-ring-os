#!/usr/bin/env python3
"""Validate run-user arguments before entering the pinned Nao task runner."""
import argparse
import os
import sys
from pathlib import Path

SCENARIOS = [
    ("hello", "SQ/CQ stdout and completion checks"),
    ("kernel-read", "Attempt to read supervisor memory"),
    ("code-write", "Attempt to write RX code"),
    ("cq-write", "Attempt to write the protected CQ"),
    ("stack-guard", "Read an unmapped stack guard"),
    ("port-io", "Attempt direct UART port I/O"),
    ("loop", "Infinite loop; CPU budget terminates it"),
    ("sq-fault", "Invalid SQ cursor disables the pair"),
    ("cq-fault", "Invalid CQ cursor disables the pair"),
    ("bad-stack", "Reject a zero syscall return stack"),
    ("boot-write", "Attempt to write bootstrap metadata"),
    ("nx-stack", "Attempt to execute stack memory"),
    ("cq-full", "CQ backpressure and drain"),
    ("registers", "General/SIMD/MXCSR preservation"),
    ("apic-write", "Attempt to write supervisor APIC registers"),
    ("sq-head-write", "Attempt to write the kernel SQ consumer"),
    ("noncanonical-stack", "Reject a noncanonical syscall return stack"),
    ("task-a", "CPU-bound preemption, private data and register checks"),
    ("task-b", "Second CPU-bound preemption and queue instance"),
]


def main():
    parser = argparse.ArgumentParser(prog="./n run-user", description="Run one or two isolated userspace scenarios.")
    parser.add_argument("selection", nargs="?", help="scenario number or name")
    parser.add_argument("--scenario", help="scenario number or name")
    parser.add_argument("--peer", help="optional second process scenario number or name")
    parser.add_argument("--list-scenarios", action="store_true")
    arguments = sys.argv[1:]
    if arguments[:1] == ["--"]: arguments = arguments[1:]
    args = parser.parse_args(arguments)
    if args.list_scenarios:
        for index, (name, description) in enumerate(SCENARIOS):
            print(f"{index:2}  {name:20} {description}")
        return
    if args.selection is not None and args.scenario is not None:
        parser.error("select a scenario once")
    names = [name for name, _ in SCENARIOS]
    def selection(value):
        if value in names: value = str(names.index(value))
        if not value.isascii() or not value.isdecimal() or not 0 <= int(value) < len(SCENARIOS):
            parser.error("unknown scenario; use ./n run-user --list-scenarios")
        return str(int(value))
    os.environ["BYTE_RING_USER_SCENARIO"] = selection(args.scenario or args.selection or "0")
    os.environ["BYTE_RING_USER_PEER"] = selection(args.peer) if args.peer is not None else ""
    root = Path(__file__).resolve().parent.parent
    os.chdir(root)
    os.execv("./t", ["./t", "nao", "run-user"])


if __name__ == "__main__":
    main()
