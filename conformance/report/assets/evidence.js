/* Human-readable views of recorded evidence. Never render transport envelopes. */
(() => {
  const labels = {
    full_sql: ['✓', 'Fully lowered to SQL'],
    mixed: ['◐', 'SQL + OrchidDB runtime'],
    runtime: ['○', 'OrchidDB runtime'],
    unknown: ['?', 'Plan not captured'],
  };
  const node = (tag, text, className) => {
    const element = document.createElement(tag);
    if (text !== undefined) element.textContent = text;
    if (className) element.className = className;
    return element;
  };
  const human = key => key.replace(/^\$/, '').replace(/[_-]/g, ' ').replace(/^./, c => c.toUpperCase());
  const present = value => value !== undefined && value !== null && value !== '';
  function badge(state) {
    const [icon, text] = labels[state] || labels.unknown;
    const result = node('span', undefined, `sql-lowering ${labels[state] ? state : 'unknown'}`);
    const symbol = node('span', icon); symbol.setAttribute('aria-hidden', 'true');
    result.append(symbol, document.createTextNode(text));
    return result;
  }
  function facts(parent, items) {
    const list = node('dl', undefined, 'evidence-facts');
    for (const [label, value] of items.filter(([, value]) => present(value))) {
      const item = node('div'); item.append(node('dt', label));
      const detail = node('dd'); readable(detail, value); item.append(detail); list.append(item);
    }
    if (list.children.length) parent.append(list);
  }
  // Nested values become labeled fields/lists, including graph values and typed terms.
  function readable(parent, value) {
    if (value === null || value === undefined) { parent.append(node('span', 'null', 'null-value')); return; }
    if (typeof value !== 'object') { parent.append(node('span', String(value))); return; }
    if (Array.isArray(value)) {
      if (!value.length) { parent.append(node('span', 'Empty list', 'null-value')); return; }
      const list = node('ul', undefined, 'value-list');
      for (const item of value) { const li = node('li'); readable(li, item); list.append(li); }
      parent.append(list); return;
    }
    if (value['@type'] && '@value' in value) {
      parent.append(node('small', value['@type'].replace(/^g:/, ''), 'value-type'));
      readable(parent, value['@value']); return;
    }
    if (value.type && 'value' in value) {
      parent.append(node('small', value.type, 'value-type')); readable(parent, value.value); return;
    }
    const entries = Object.entries(value);
    if (!entries.length) { parent.append(node('span', 'Empty map', 'null-value')); return; }
    facts(parent, entries.map(([key, item]) => [human(key), item === null ? 'null' : item]));
  }
  function section(parent, title) {
    const element = node('section', undefined, 'evidence-section');
    element.append(node('h3', title)); parent.append(element); return element;
  }
  function code(parent, title, text, sql = false) {
    if (!present(text)) return;
    const container = section(parent, title), pre = node('pre'), content = node('code');
    if (sql) {
      content.className = 'sql-code';
      // Preserve every captured character. CSS adds line breaks at SQL boundaries;
      // strings, identifiers and comments are indivisible, even if they contain SQL.
      const tokens = String(text).match(/\$(?:[A-Za-z_][A-Za-z_0-9]*)?\$[\s\S]*?\$(?:[A-Za-z_][A-Za-z_0-9]*)?\$|'(?:''|\\[\s\S]|[^'])*'|"(?:""|[^"])*"|`(?:``|[^`])*`|--[^\n]*(?:\n|$)|\/\*[\s\S]*?\*\/|\s+|[A-Za-z_][A-Za-z_0-9]*|[\s\S]/g) || [];
      let depth = 0, afterComma = false;
      const boundaries = new Set(['SELECT', 'FROM', 'WHERE', 'GROUP', 'ORDER', 'HAVING', 'LIMIT', 'OFFSET', 'UNION', 'EXCEPT', 'INTERSECT', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'FULL', 'CROSS', 'ON', 'WITH', 'RETURNING']);
      for (const token of tokens) {
        const span = node('span', token), word = token.toUpperCase();
        if (token === ')') depth = Math.max(0, depth - 1);
        if (!/^\s+$/.test(token)) {
          if ((depth === 0 && boundaries.has(word)) || (depth === 0 && afterComma)) span.classList.add('sql-line');
          afterComma = false;
        }
        if (boundaries.has(word) || ['AS', 'AND', 'OR', 'NULL', 'DISTINCT', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END'].includes(word)) span.classList.add('sql-keyword');
        if (token === '(') depth++;
        if (token === ',' && depth === 0) afterComma = true;
        content.append(span);
      }
    } else content.textContent = text;
    pre.append(content); container.append(pre);
  }
  function resultTable(parent, title, value, columns = []) {
    if (value === undefined || value === null) return;
    const container = section(parent, title);
    let rows = value;
    if (!Array.isArray(rows)) rows = value.rows ?? value.native_rows ?? value.bindings ?? value.results?.bindings;
    if (!Array.isArray(rows)) { readable(container, value); return; }
    if (!rows.length) { container.append(node('p', 'No rows returned.')); return; }
    const isRecord = row => row && !Array.isArray(row) && typeof row === 'object';
    if (!columns.length && rows.every(isRecord)) columns = [...new Set(rows.flatMap(row => Object.keys(row)))];
    if (!columns.length) columns = Array.from({length: Math.max(...rows.map(row => Array.isArray(row) ? row.length : 1))}, (_, i) => `Value ${i + 1}`);
    const table = node('table', undefined, 'result-table'), head = node('thead'), headings = node('tr'), body = node('tbody');
    for (const column of columns) { const th = node('th', column); th.scope = 'col'; headings.append(th); }
    head.append(headings);
    for (const row of rows) {
      const tr = node('tr');
      const values = Array.isArray(row) ? row : isRecord(row) ? columns.map(column => row[column]) : [row];
      for (const value of values) { const cell = node('td'); readable(cell, value); tr.append(cell); }
      body.append(tr);
    }
    table.append(head, body); const scroll = node('div', undefined, 'result-scroll'); scroll.tabIndex = 0; scroll.append(table); container.append(scroll);
  }
  function stateOf(plan) {
    if (plan?.format_version !== 1 || plan.capture !== 'executed') return 'unknown';
    if (plan.lowering === 'full_sql' && plan.fully_lowered_to_sql !== true) return 'unknown';
    return labels[plan.lowering] ? plan.lowering : 'unknown';
  }
  function executionPlans(parent, data) {
    const queries = (data.query_cost?.queries || []).filter(query => query && typeof query === 'object');
    const tested = queries.filter(query => ['query', 'control'].includes(query.phase || 'query'));
    const helpers = queries.filter(query => !tested.includes(query));
    if (!tested.length) {
      code(parent, 'Original query', data.query);
      const panel = section(parent, 'OrchidDB plan and SQL'); panel.append(badge('unknown'));
      panel.append(node('p', 'This run did not capture an execution plan. Rerun the scenario to record its plan and generated SQL.'));
    }
    function renderQuery(target, query, index, helper = false) {
      const plan = query.cost?.execution_plan, state = stateOf(plan);
      const panel = node(helper ? 'details' : 'section', undefined, 'query-plan');
      panel.append(node(helper ? 'summary' : 'h3', helper ? `${human(query.phase)} ${index + 1}` : `Query ${index + 1}`));
      panel.append(badge(state)); target.append(panel);
      code(panel, 'Original query', query.query);
      if (query.step) facts(panel, [['Scenario step', query.step]]);
      if (state === 'unknown') {
        panel.append(node('p', 'Execution plan not captured. Syntax checks and failed requests may not return execution statistics.')); return;
      }
      const reads = plan.sql_queries || [], writes = plan.write_queries || [];
      const count = reads.length + writes.length, residual = plan.residual_operators;
      const engines = Object.entries(plan.sql_regions || {}).filter(([, count]) => count > 0).map(([name, count]) => `${count} ${name === 'duckdb' ? 'DuckDB' : name === 'postgres' ? 'PostgreSQL' : 'other'} region${count === 1 ? '' : 's'}`);
      facts(panel, [['SQL statements', count], ['Execution engines', engines.join(' · ')], ['Runtime operators remaining', residual], ['Storage', plan.storage === 'mapped_relational_tables' ? 'Mapped relational tables' : undefined], ['Tables', plan.tables?.join(', ')]]);
      if (plan.schema_sql?.length) {
        const schema = node('details', undefined, 'relational-schema');
        schema.append(node('summary', 'SQL schema'));
        code(schema, 'Tables used by this scenario', plan.schema_sql.join('\n\n'), true);
        panel.append(schema);
      }
      code(panel, 'OrchidDB logical plan', plan.logical_plan);
      code(panel, 'OrchidDB physical plan', plan.physical_plan);
      reads.forEach((sql, i) => code(panel, reads.length === 1 ? 'Generated SQL' : `Generated SQL · statement ${i + 1}`, sql, true));
      writes.forEach((statement, i) => {
        code(panel, writes.length === 1 ? 'Write SQL' : `Write SQL · statement ${i + 1}`, statement.sql, true);
        if (statement.inputs?.length) facts(panel, [['Write input', statement.inputs.map(([name, rows]) => `${name}: ${rows} row${rows === 1 ? '' : 's'}`).join(' · ')]]);
      });
      if (!count) panel.append(node('p', 'No SQL was generated. This query ran through OrchidDB runtime operators.', 'execution-note'));
      (plan.native_source_queries || []).forEach((sql, i) => code(panel, `Source SQL for runtime operators · statement ${i + 1}`, sql, true));
    }
    tested.forEach((query, i) => renderQuery(parent, query, i));
    if (helpers.length) {
      const details = node('details', undefined, 'helper-queries');
      details.append(node('summary', `Setup and assertion helpers (${helpers.length})`));
      details.append(node('p', 'These requests do not affect the scenario’s SQL-lowering indicator.'));
      helpers.forEach((query, i) => renderQuery(details, query, i, true)); parent.append(details);
    }
  }
  function measurements(parent, data) {
    const cost = data.query_cost;
    const items = [['Scenario duration', present(data.elapsed_ms) ? `${data.elapsed_ms.toLocaleString()} ms` : undefined]];
    if (cost) items.push(['Query duration', present(cost.request_elapsed_micros) ? `${(cost.request_elapsed_micros / 1000).toLocaleString()} ms` : undefined], ['Work units', cost.work_units], ['Measurement coverage', cost.coverage ? human(cost.coverage) : undefined], ['Queries measured', cost.measured_queries], ['Helper requests excluded', cost.helper_query_count]);
    if (items.some(([, value]) => present(value))) facts(section(parent, 'Measurements'), items);
  }
  function render(parent, data, upstream, product, scenario = {}) {
    parent.replaceChildren();
    if (upstream) {
      if (data.steps) for (const step of data.steps) {
        const panel = section(parent, step.text);
        if (step.doc) { const pre = node('pre'); pre.append(node('code', step.doc)); panel.append(pre); }
        if (step.table?.length) resultTable(panel, 'Expected values', step.table.slice(1), step.table[0]);
      }
      else facts(parent, [['Test', data.name], ['Test kind', data.types], ['Query file', data.query_file], ['Expected result file', data.result_file], ['Fixture files', data.data], ['Named graphs', data.named]]);
      return;
    }
    const status = node('p', human(data.status || 'not recorded'), `case-outcome ${data.status || ''}`); parent.append(status);
    if (data.reason || data.error) readable(section(parent, 'Diagnostic'), data.reason || data.error);
    if (data.failed_step) readable(section(parent, 'Failed step'), data.failed_step);
    if (product === 'orchiddb') executionPlans(parent, data);
    else code(parent, 'Original query', data.query);
    let actual = data.actual ?? data.actual_graphson;
    if (typeof actual === 'string') { try { actual = JSON.parse(actual); } catch { /* Plain diagnostic text. */ } }
    const columns = actual?.columns || actual?.native_columns || [];
    const results = node('div', undefined, 'result-comparison');
    resultTable(results, 'Expected result', data.expected, columns);
    if (data.expected_error) readable(section(results, 'Expected error'), data.expected_error);
    if (actual?.error) {
      readable(section(results, 'Actual error'), actual.error);
      if (actual.classification) facts(results, Object.entries(actual.classification).map(([key, value]) => [human(key), value]));
      actual = undefined;
    } else if (actual && typeof actual === 'object' && !Array.isArray(actual)
               && ('query_cost' in actual || 'engine_instance' in actual || 'native_rows' in actual)) {
      actual = actual.native_rows ?? actual.rows;
    }
    resultTable(results, 'Actual result', actual, columns);
    if (results.children.length) parent.append(results);
    const source = section(parent, 'Scenario and expectation');
    if (scenario.steps?.length) {
      const steps = node('details'); steps.append(node('summary', 'Read upstream scenario'));
      render(steps.appendChild(node('div')), scenario, true); source.append(steps);
    } else facts(source, [['Test', scenario.name], ['Query file', scenario.query_file], ['Expected result file', scenario.result_file]]);
    if (scenario.source) { const link = node('a', 'Pinned upstream source ↗'); link.href = scenario.source; source.append(link); }
    measurements(parent, data);
  }
  window.OrchidEvidence = {render};
})();
