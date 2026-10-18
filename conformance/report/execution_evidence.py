"""Describe SQL lowering only from recorded execution evidence."""
from html import escape

STATES = {
    'full_sql': ('✓', 'Fully lowered to SQL'),
    'mixed': ('◐', 'SQL + OrchidDB runtime'),
    'runtime': ('○', 'OrchidDB runtime'),
    'unknown': ('?', 'Plan not captured'),
}


def queries(result):
    return [q for q in (result.get('query_cost') or {}).get('queries', [])
            if isinstance(q, dict)]


def lowering(result):
    # Setup and assertion helpers must not change the tested query's indicator.
    selected = [q for q in queries(result) if q.get('phase', 'query') in ('query', 'control')]
    states = []
    for query in selected:
        plan = (query.get('cost') or {}).get('execution_plan') or {}
        state = plan.get('lowering', 'unknown')
        if plan.get('format_version') != 1 or plan.get('capture') != 'executed':
            state = 'unknown'
        if state == 'full_sql' and plan.get('fully_lowered_to_sql') is not True:
            state = 'unknown'
        states.append(state if state in STATES else 'unknown')
    if not states or 'unknown' in states:
        return 'unknown'
    if all(state == 'full_sql' for state in states):
        return 'full_sql'
    if all(state == 'runtime' for state in states):
        return 'runtime'
    return 'mixed'


def badge(state):
    icon, label = STATES.get(state, STATES['unknown'])
    return (f'<span class="sql-lowering {escape(state)}" title="{escape(label)}">'
            f'<span aria-hidden="true">{icon}</span> {escape(label)}</span>')
