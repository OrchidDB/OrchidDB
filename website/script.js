(function () {
  const examples = {
    cypher: {
      label: "CYPHER INPUT",
      query: `<span class="kw">MATCH</span> (p:Person)-[:KNOWS]-&gt;(f)\n<span class="kw">WHERE</span> f.city = <span class="str">'Chicago'</span>\n<span class="kw">RETURN</span> p.name, <span class="fn">count</span>(f)`,
      ir: [["GraphNodeScan", ":Person", 0], ["GraphExpand", ":KNOWS · both", 1], ["GraphFilter", "city = Chicago", 2], ["GraphAggregate", "SQL island 01", 3]],
      outputLabel: "DUCKDB SQL",
      sql: `<span class="kw">SELECT</span> p.name, <span class="fn">count</span>(f.id) …\n<span class="kw">FROM</span> person p <span class="kw">JOIN</span> knows e …`
    },
    gremlin: {
      label: "GREMLIN INPUT",
      query: `g.V().<span class="fn">hasLabel</span>(<span class="str">'person'</span>)\n .<span class="fn">out</span>(<span class="str">'knows'</span>).<span class="fn">has</span>(<span class="str">'city'</span>, <span class="str">'Chicago'</span>)\n .<span class="fn">groupCount</span>().<span class="fn">by</span>(<span class="str">'name'</span>)`,
      ir: [["GraphNodeScan", ":person", 0], ["GraphExpand", ":knows · out", 1], ["GraphFilter", "city = Chicago", 2], ["GraphGroupMap", "SQL island 02", 3]],
      outputLabel: "DUCKDB SQL",
      sql: `<span class="kw">SELECT</span> f.name, <span class="fn">count</span>(*) …\n<span class="kw">FROM</span> person p <span class="kw">JOIN</span> knows e …`
    },
    sparql: {
      label: "SPARQL INPUT",
      query: `<span class="kw">PREFIX</span> ex: &lt;https://example.com/&gt;\n<span class="kw">SELECT</span> ?person ?name <span class="kw">WHERE</span> {\n  ?person a ex:Person ; ex:name ?name .\n}`,
      ir: [["OntologyResolve", "ex:Person → :Person", 0], ["GraphNodeScan", ":Person", 1], ["GraphProject", "ex:name → name", 2], ["GraphReturn", "SQL island 03", 3]],
      outputLabel: "DUCKDB SQL",
      sql: `<span class="kw">SELECT</span> p.person_id, p.full_name\n<span class="kw">FROM</span> people_view p`
    },
    recursive: {
      label: "CYPHER · VARIABLE LENGTH",
      query: `<span class="kw">MATCH</span> (a:Account)-[:TRANSFER*1..]-&gt;(b)\n<span class="kw">WHERE</span> b.risk &gt; <span class="str">0.8</span>\n<span class="kw">RETURN</span> a.id, b.id`,
      ir: [["GraphNodeScan", ":Account", 0], ["GraphExpand", ":TRANSFER · 1..", 1], ["GraphPathFilter", "trail semantics", 2], ["GraphProject", "recursive SQL", 3]],
      outputLabel: "DUCKDB SQL",
      sql: `<span class="kw">WITH RECURSIVE</span> walk <span class="kw">AS</span> (…)\n<span class="kw">SELECT</span> src_id, dst_id <span class="kw">FROM</span> walk …`
    }
  };

  const tabs = [...document.querySelectorAll("[data-example]")];
  const label = document.querySelector("#input-label");
  const query = document.querySelector("#query-code");
  const ir = document.querySelector("#ir-rows");
  const sql = document.querySelector("#sql-code");
  const outputLabel = document.querySelector("#output-label");

  function selectExample(tab) {
    const example = examples[tab.dataset.example];
    tabs.forEach((item) => {
      const selected = item === tab;
      item.setAttribute("aria-selected", String(selected));
      item.tabIndex = selected ? 0 : -1;
    });
    label.textContent = example.label;
    query.innerHTML = example.query;
    ir.innerHTML = `<p class="stage-label"><span>02</span> GRAPH IR</p>` + example.ir.map(([op, note, depth], index) =>
      `<div class="ir-row ${depth ? `indent${depth > 1 ? `-${depth}` : ""}` : ""} ${index === example.ir.length - 1 ? "hot" : ""}"><i></i><code>${op}</code><small>${note}</small></div>`
    ).join("");
    outputLabel.textContent = example.outputLabel;
    sql.innerHTML = example.sql;
  }

  tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => selectExample(tab));
    tab.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      const offset = event.key === "ArrowRight" ? 1 : -1;
      const next = tabs[(index + offset + tabs.length) % tabs.length];
      selectExample(next);
      next.focus();
    });
  });

  const byosExamples = {
    cypher: {
      query: `<span class="kw">MATCH</span> (a:Account)-[t:TRANSFERRED_TO]-&gt;(b:Account)\n<span class="kw">RETURN</span> a.owner, b.owner, t.amount`,
      sql: `<span class="kw">SELECT</span> a.owner, b.owner, t.amount\n<span class="kw">FROM</span> graph_accounts a\n<span class="kw">JOIN</span> graph_transfers t <span class="kw">ON</span> t.from_id = a.account_id\n<span class="kw">JOIN</span> graph_accounts b <span class="kw">ON</span> b.account_id = t.to_id`
    },
    gremlin: {
      query: `g.V().<span class="fn">hasLabel</span>(<span class="str">'Account'</span>).<span class="fn">as</span>(<span class="str">'a'</span>)\n .<span class="fn">outE</span>(<span class="str">'TRANSFERRED_TO'</span>).<span class="fn">as</span>(<span class="str">'t'</span>).<span class="fn">inV</span>().<span class="fn">as</span>(<span class="str">'b'</span>)\n .<span class="fn">select</span>(<span class="str">'a'</span>, <span class="str">'b'</span>, <span class="str">'t'</span>)`,
      sql: `<span class="kw">SELECT</span> a.*, b.*, t.*\n<span class="kw">FROM</span> graph_accounts a\n<span class="kw">JOIN</span> graph_transfers t <span class="kw">ON</span> t.from_id = a.account_id\n<span class="kw">JOIN</span> graph_accounts b <span class="kw">ON</span> b.account_id = t.to_id`
    },
    sparql: {
      query: `<span class="kw">PREFIX</span> ex: &lt;https://crabgraph.net/schema/&gt;\n<span class="kw">SELECT</span> ?account ?owner <span class="kw">WHERE</span> {\n  ?account a ex:Account ; ex:owner ?owner .\n}`,
      sql: `<span class="kw">SELECT</span> a.account_id <span class="kw">AS</span> <span class="str">"account"</span>, a.owner <span class="kw">AS</span> <span class="str">"owner"</span>\n<span class="kw">FROM</span> graph_accounts a`
    }
  };
  const byosTabs = [...document.querySelectorAll("[data-byos-language]")];
  const byosQuery = document.querySelector("#byos-query");
  const byosSql = document.querySelector("#byos-sql");
  function selectByos(tab) {
    const example = byosExamples[tab.dataset.byosLanguage];
    byosTabs.forEach((item) => {
      const selected = item === tab;
      item.setAttribute("aria-selected", String(selected));
      item.tabIndex = selected ? 0 : -1;
    });
    byosQuery.innerHTML = example.query;
    byosSql.innerHTML = example.sql;
  }
  byosTabs.forEach((tab, index) => {
    tab.addEventListener("click", () => selectByos(tab));
    tab.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      const offset = event.key === "ArrowRight" ? 1 : -1;
      const next = byosTabs[(index + offset + byosTabs.length) % byosTabs.length];
      selectByos(next);
      next.focus();
    });
  });

  const button = document.querySelector("[data-copy]");
  if (!button) return;

  button.addEventListener("click", async () => {
    const original = button.textContent;
    try {
      await navigator.clipboard.writeText(button.dataset.copy);
      button.textContent = "Copied";
    } catch {
      button.textContent = "Copy failed";
    }
    window.setTimeout(() => { button.textContent = original; }, 1600);
  });
}());
