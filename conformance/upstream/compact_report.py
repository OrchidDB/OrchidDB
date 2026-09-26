"""Create small publishable results; keep full execution payloads under target/."""
import argparse
import json
from pathlib import Path


def compact(report):
    result = {k: v for k, v in report.items() if k not in ('results', 'environment')}
    result['evidence_format'] = 'compact-v1'
    key = 'results' if 'results' in report else 'cases'
    if key not in report or not isinstance(report[key], list):
        return result
    result[key] = []
    for row in report[key]:
        if not isinstance(row, dict) or 'status' not in row:
            result[key].append(row)
            continue
        entry = {k: row[k] for k in ('id', 'case_sha256', 'normalized_case_sha256',
                 'status', 'elapsed_ms', 'execution_profile', 'reason', 'error', 'source_sha256') if k in row}
        for diagnostic in ('reason', 'error'):
            if isinstance(entry.get(diagnostic), str):
                entry[diagnostic] = entry[diagnostic][:1000]
        if 'query_cost' in row:
            entry['query_cost'] = {k: v for k, v in row['query_cost'].items() if k != 'queries'}
        result[key].append(entry)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(json.dumps(compact(json.loads(args.source.read_text())),
                                     separators=(',', ':')) + '\n')
