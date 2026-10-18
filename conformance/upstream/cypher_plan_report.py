"""Summarize a plan-audit journal without treating plan checks as conformance."""
import argparse
from collections import Counter
import html
import json
from pathlib import Path


def summarize(directory):
    findings = Counter()
    families = Counter()
    errors = {}
    largest = []
    inspected = 0
    decoded = 0
    for line in (directory/'results.jsonl').open():
        # Only an unfinished final record may be ignored in a live journal.
        if not line.endswith('\n'):
            break
        row = json.loads(line)
        inspected += 1
        families[row['family']] += 1
        findings.update(row['findings'])
        decoded += bool(row['metrics'].get('output_decode_only'))
        result = row['result']
        messages = [result['error']] if result.get('error') else []
        messages += [plan['error'] for plan in result.get('sql_plans', []) if plan.get('error')]
        for message in messages:
            key = message.split('\n')[0]
            if key not in errors:
                errors[key] = dict(count=0, families=Counter(), modes=Counter(), example=row)
            errors[key]['count'] += 1
            errors[key]['families'][row['family']] += 1
            errors[key]['modes'][row['mode']] += 1
        largest.append(dict(id=row['id'], mode=row['mode'], family=row['family'],
                            sql_chars=row['metrics']['sql_chars'], query=row['request']['query']))
        largest.sort(key=lambda r: r['sql_chars'], reverse=True)
        del largest[25:]
    manifest_path = directory/'manifest.json'
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else None
    complete = bool(manifest and manifest.get('inspections') == inspected)
    summary = dict(complete=complete, inspected=inspected, target_queries_executed=False,
                   findings=dict(findings), families=dict(families), output_decode_only=decoded,
                   error_groups=errors, largest_sql=largest)
    (directory/'review.json').write_text(json.dumps(summary, indent=2)+'\n')
    escape = html.escape
    sections = []
    for message, group in sorted(errors.items(), key=lambda item:-item[1]['count']):
        row = group['example']; result = row['result']
        sql = [result['sql']] if result.get('sql') else [r['sql'] for r in result.get('regions', [])]
        sections.append(f'<section><h2>{escape(message)}</h2><p>{group["count"]} occurrences; '
                        f'{escape(str(dict(group["families"])))}; {escape(str(dict(group["modes"])))}'
                        f'</p><p>Reproduction: {row["id"]}, {row["mode"]}, {row["fixture"]}</p>'
                        f'<h3>Cypher</h3><pre>{escape(row["request"]["query"])}</pre>'
                        f'<h3>Fixture</h3><pre>{escape(chr(10).join(row["request"]["setup"]))}</pre>'
                        f'<h3>Generated SQL</h3><pre>{escape(chr(10).join(sql)) or "No SQL emitted"}</pre></section>')
    document = '<!doctype html><html><meta charset="utf-8"><title>Cypher DuckDB plan audit</title><style>body{max-width:1100px;margin:40px auto;font:16px system-ui;padding:0 20px}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#f3f5f7;padding:18px}section{border-top:1px solid #aaa;margin-top:32px}h2{font-size:20px}</style>'
    document += f'<h1>Cypher → DuckDB plan audit</h1><p>{inspected:,} inspections. Status: {"complete" if complete else "in progress"}.</p><p>Target queries were not executed. Binding failures are actionable; cardinality estimates and expensive shapes require review. These results do not establish semantic conformance.</p>'
    document += '<pre>'+escape(json.dumps(dict(findings),indent=2))+'</pre>'+''.join(sections)+'</html>'
    (directory/'review.html').write_text(document)
    print(json.dumps({k:v for k,v in summary.items() if k not in ('error_groups','largest_sql')}))

if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    summarize(parser.parse_args().directory)
