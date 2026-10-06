"""Reuse the pinned original JUnit provider and verify extension attribution."""
import hashlib
import json
from pathlib import Path


def original_provider_cases(repo, catalog, source):
    manifest_path = repo / 'conformance/adapters/jvm-provider-tests/placeholders.json'
    manifest = json.loads(manifest_path.read_text())
    if manifest['upstream_revision'] != catalog['sources']['tinkerpop']['revision']:
        raise ValueError('Original JUnit provider revision does not match the pinned catalog')
    placeholders = {
        case['id']: case for case in catalog['cases'] if case['suite'] == 'tinkerpop'
        and any(step['text'] == 'an unsupported test' for step in case['steps'])
    }
    mapped = {}
    for entry in manifest['cases']:
        ids = [case_id for case_id in placeholders if case_id.endswith('/' + entry['gherkin'])]
        if len(ids) != 1 or ids[0] in mapped:
            raise ValueError('Original JUnit provider mapping is missing or ambiguous: ' + entry['gherkin'])
        feature = 'gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/' + entry['gherkin'].split(':')[0]
        for relative, expected in ((entry['source'], entry['source_sha256']),
                                   (feature, entry['feature_source_sha256'])):
            if hashlib.sha256((source / relative).read_bytes()).hexdigest() != expected:
                raise ValueError('Pinned original assertion source changed: ' + relative)
        mapped[ids[0]] = entry
    if mapped.keys() != placeholders.keys():
        raise ValueError('Every placeholder must map to its original upstream JUnit method')
    return mapped


def verify_provider_result(result, entry, artifact):
    """Never manufacture a pass. Invalid pass evidence becomes an adapter error."""
    if result.get('status') != 'pass':
        return result
    evidence = result.get('java_assertion') or {}
    expected = {'class': entry['java_class'], 'method': entry['java_method'],
                'source': entry['source'], 'source_sha256': entry['source_sha256'],
                'feature_source_sha256': entry['feature_source_sha256'],
                'run_count': 1, 'failure_count': 0, 'ignored_count': 0, 'assumption_count': 0}
    error = None
    if any(evidence.get(key) != value for key, value in expected.items()):
        error = 'Missing or invalid original JUnit execution evidence'
    transports = result.get('query_transports', [])
    instance = result.get('engine_instance')
    if not transports or not instance:
        error = 'Original JUnit method made no attributable extension requests'
    for transport in transports:
        if (transport.get('backend') != 'duckdb-extension'
                or transport.get('engine_instance') != instance
                or transport.get('extension_artifact_sha256') != artifact):
            error = 'Original JUnit traversal did not use the pinned extension instance/artifact'
    if error:
        return {**result, 'status': 'adapter-error', 'error': error}
    return result
