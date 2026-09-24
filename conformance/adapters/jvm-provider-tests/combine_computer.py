#!/usr/bin/env python3
"""Combine disjoint original computer-suite runs with matching frozen runtimes."""
import argparse
import collections
import copy
import datetime
import hashlib
import json
from pathlib import Path


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--broad', type=Path, required=True)
    parser.add_argument('--algorithms', type=Path, required=True)
    parser.add_argument('--inventory', type=Path, help='Defaults to broad report directory inventory.json')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    broad = json.loads(args.broad.read_text())
    algorithms = json.loads(args.algorithms.read_text())
    inventory = json.loads((args.inventory or args.broad.parent / 'inventory.json').read_text())
    require(broad['run_complete'] and algorithms['run_complete'], 'Incomplete component run')
    require(not broad.get('inventory_only') and not algorithms.get('inventory_only'), 'Inventory is not execution evidence')
    require(broad['profile'] == algorithms['profile'] == 'jvm-graphcomputer', 'Execution profile mismatch')
    require(broad['upstream_revision'] == algorithms['upstream_revision'] == inventory['upstream_revision'], 'Upstream revision mismatch')
    require(broad['store_sha256'] == algorithms['store_sha256'], 'Native binary mismatch')
    require(broad['runtime_jar_sha256'] == algorithms['provider_jar_sha256'], 'JVM runtime mismatch')
    require(algorithms['effective_jvm_properties']['is.testing'] == 'true' and
            algorithms['effective_jvm_properties']['assertNonDeterministic'] == 'true', 'Algorithm test flag mismatch')
    require(len(broad['cases']) == 837 and len(algorithms['cases']) == 31, 'Unexpected component selection sizes')
    cases = copy.deepcopy(broad['cases']) + copy.deepcopy(algorithms['cases'])
    require(collections.Counter(row['id'] for row in cases) ==
            collections.Counter(row['id'] for row in inventory['cases']), 'Suite occurrence mismatch')
    source_hashes = {row['id']: row['source_sha256'] for row in inventory['cases']}
    require(all(row['source_sha256'] == source_hashes[row['id']] for row in cases), 'Original source hash mismatch')
    byclass, index, last = {}, -1, None
    for row in inventory['cases']:
        klass = row['id'].split('#')[0]
        if klass != last:
            index += 1
            last = klass
        byclass.setdefault(klass, index)
    for row in cases:
        if 'suite_class_index' not in row:
            row.update(suite_class_index=byclass[row['id'].split('#')[0]],
                       suite_class_occurrence=1, component_report=str(args.algorithms.resolve()))
    cases.sort(key=lambda row: (row['suite_class_index'], row['id']))
    report = {key: copy.deepcopy(value) for key, value in broad.items() if key not in (
        'cases', 'counts', 'excluded_classes_owned_by_parallel_original31_run', 'selected_occurrences')}
    report.update(selection='ProcessComputerSuite', selected_occurrences=len(cases),
                  counts=dict(collections.Counter(row['status'] for row in cases)),
                  unique_case_ids=len({row['id'] for row in cases}), cases=cases)
    report['duplicate_declared_occurrences'] = len(cases) - report['unique_case_ids']
    unique_outcomes = collections.defaultdict(set)
    for row in cases:
        unique_outcomes[row['id']].add(row['status'])
    report['unique_case_counts'] = dict(collections.Counter(
        next(iter(statuses)) if len(statuses) == 1 else 'mixed'
        for statuses in unique_outcomes.values()))
    report['mixed_repeat_outcomes'] = {case_id: sorted(statuses)
                                       for case_id, statuses in unique_outcomes.items() if len(statuses) > 1}
    exclusions = collections.Counter()
    for row in cases:
        if row['status'] != 'skipped':
            continue
        reason = row.get('reason', '')
        if reason == 'JUnit @Ignore':
            category = 'upstream_junit_ignore'
        elif 'is ignored for COMPUTER' in reason:
            category = 'upstream_computer_guard'
        elif 'apply to TinkerGraph only' in reason:
            category = 'upstream_tinkergraph_only_guard'
        elif 'feature' in reason.lower():
            category = 'feature_requirement'
        else:
            category = 'other_upstream_assumption'
        exclusions[category] += 1
    report['exclusion_occurrence_counts'] = dict(exclusions)
    report['component_reports'] = [
        {'path': str(path.resolve()), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'occurrences': count}
        for path, count in ((args.broad, 837), (args.algorithms, 31))]
    report['combined_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    report['timing_scope'] = 'started_at and finished_at describe the broad component; algorithm execution is independently recorded'
    report['algorithm_component_provenance'] = {key: value for key, value in algorithms.items() if key not in ('cases', 'counts')}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['counts']))


if __name__ == '__main__':
    main()
