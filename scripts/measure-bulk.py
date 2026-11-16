#!/usr/bin/env python3
"""Time representative CLI commands on a generated, disposable Package.

The Package holds fictional Orders and lives in a temporary folder that is deleted on
exit, so no owner data is read and no load outlives the run. Times are wall-clock seconds
for the release build, the median of a few runs each.
"""

import argparse
import json
import os
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import tempfile
import time
import uuid


ROOT = Path(__file__).resolve().parent.parent


def generate(root, count):
    """Writes `count` fictional Orders and a Vendor record into the Package at `root`."""
    rng = random.Random(7)
    vendors = [
        {"vendor_id": str(uuid.UUID(int=rng.getrandbits(128), version=4)),
         "name": f"Supplier {index:02d}", "contacts": []}
        for index in range(40)
    ]
    orderers = [f"Person {index}" for index in range(25)]
    projects = [f"Project {index}" for index in range(12)]
    (root / ".spielorders/vendors.json").write_text(
        json.dumps({"schema_version": 1, "vendors": vendors}, indent=2) + "\n")
    for index in range(count):
        title = f"Order {index:05d}"
        date = f"20{10 + index % 16:02d}-{1 + index % 12:02d}-{1 + index % 28:02d}"
        directory = root / "Orders" / f"{date.replace('-', '_')}_{title.replace(' ', '_')}"
        (directory / ".spielorders").mkdir(parents=True)
        order = {
            "schema_version": 1, "package_root": "../..",
            "order_id": str(uuid.UUID(int=rng.getrandbits(128), version=4)),
            "title": title, "date_ordered": date, "status": "complete",
            "vendor_id": rng.choice(vendors)["vendor_id"], "orderer": rng.choice(orderers),
            "project": rng.choice(projects), "order_group": f"Group {index % 5}",
            "items": [{"description": f"Part {item}", "quantity": 1.0,
                       "unit_price": 10.0 + item} for item in range(3)],
        }
        (directory / ".spielorders/order.json").write_text(json.dumps(order, indent=2) + "\n")
        (directory / "README.md").write_text(f"# {title}\n")


def timed(binary, samples):
    """The median seconds of concrete samples; mutation JSON must report a real change."""
    seconds = []
    for arguments in samples:
        start = time.perf_counter()
        result = subprocess.run(
            [binary, *arguments], check=True,
            stdout=subprocess.PIPE if "--json" in arguments else subprocess.DEVNULL,
        )
        seconds.append(time.perf_counter() - start)
        if result.stdout is not None:
            outcome = json.loads(result.stdout)["data"]["outcome"]
            if outcome not in ("committed", "committed_with_warning"):
                raise RuntimeError(f"timing sample did not change the Package: {outcome}")
    return statistics.median(seconds)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--orders", type=int, default=3000)
    parser.add_argument("--runs", type=int, default=3)
    options = parser.parse_args()
    subprocess.run(["cargo", "build", "-q", "--release", "-p", "spielorders-cli"],
                   cwd=ROOT, check=True)
    binary = str(ROOT / "target/release/spielorders")
    folder = Path(tempfile.mkdtemp(prefix="spielorders-measure-"))
    try:
        package = folder / "Package"
        package.mkdir()
        subprocess.run([binary, "init", "--root", str(package)], check=True,
                       stdout=subprocess.DEVNULL)
        generate(package, options.orders)
        subprocess.run([binary, "index", "rebuild", "--root", str(package)], check=True,
                       stdout=subprocess.DEVNULL)
        order = sorted((package / "Orders").iterdir())[1]
        root = ["--root", str(package)]
        commands = [
            ("package show", [["package", "show", *root]] * options.runs),
            ("package list orders", [["package", "list", "orders", *root]] * options.runs),
            ("order show", [["order", "show", "--root", str(order)]] * options.runs),
            ("order edit", [
                ["order", "edit", f"title=Edited order {sample}", "--root", str(order), "--json"]
                for sample in range(options.runs)
            ]),
            ("vendor edit", [
                ["vendor", "edit", "Supplier 01", f"phone=+1 555 {100 + sample:04d}", *root, "--json"]
                for sample in range(options.runs)
            ]),
            ("value add", [
                ["value", "add", "orderer", f"Visiting Person {sample}", *root, "--json"]
                for sample in range(options.runs)
            ]),
        ]
        print(f"{options.orders} Orders, median of {options.runs} runs")
        print("Reads use human output; mutations use JSON and must change the Package on every run.")
        for name, samples in commands:
            print(f"{name:24} {timed(binary, samples):7.3f} s")
        directories = sorted((package / "Orders").iterdir())[: min(1000, options.orders)]
        operations = folder / "operations.jsonl"
        operations.write_text("".join(
            json.dumps(["order", "edit", f"title=Batch edit {index}", "--root", str(directory)])
            + "\n"
            for index, directory in enumerate(directories)
        ))
        print(f"{'apply ' + str(len(directories)) + ' edits':24} "
              f"{timed(binary, [['apply', str(operations), *root, '--json']]):7.3f} s")
    finally:
        shutil.rmtree(folder)


if __name__ == "__main__":
    main()
