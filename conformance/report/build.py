#!/usr/bin/env python3
"""Build the published conformance-report.html page locally; never run tests or publish."""
import argparse
from pathlib import Path
import shutil

from conformance_page import render

HERE = Path(__file__).resolve().parent


def build(output, results=None):
    output.mkdir(parents=True, exist_ok=True)
    body, _ = render(output, results)
    shutil.copytree(HERE / 'assets', output / 'assets', dirs_exist_ok=True)
    page = output / 'conformance-report.html'
    page.write_text('''<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Conformance report · OrchidDB</title>
<link rel="icon" href="assets/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="assets/conformance.css">
<link rel="stylesheet" href="assets/report.css">
<script src="assets/evidence.js" defer></script>
<script src="assets/conformance.js" defer></script></head>
<body class="comparison-page"><a class="skip-link" href="#content">Skip to content</a>
<header class="report-header"><a href="https://docs.orchiddb.com/conformance.html">← Documentation</a>
<a href="https://orchiddb.com/">OrchidDB</a>
<a href="https://github.com/OrchidDB/OrchidDB">GitHub</a></header>
<main id="content"><h1>Conformance report</h1>
<p>Recorded upstream scenarios, individual outcomes, execution plans, and generated SQL.</p>
''' + body + '\n</main></body></html>\n')
    return page


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=HERE.parents[1] / 'target/conformance/report')
    parser.add_argument('--results', type=Path, help='Directory of recorded <engine>-<suite>.json runs')
    args = parser.parse_args()
    print(build(args.output, args.results))
