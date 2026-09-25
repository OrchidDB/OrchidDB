import com.google.gson.*;
import org.apache.jena.graph.*;
import org.apache.jena.query.*;
import org.apache.jena.sparql.core.Quad;
import org.apache.jena.tdb2.TDB2Factory;
import org.apache.jena.update.*;
import org.apache.jena.datatypes.TypeMapper;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.TimeUnit;

/** Executes W3C inputs against unmodified Jena TDB2. Assertions live in Python. */
public final class JenaAdapter {
    private static final Gson JSON = new GsonBuilder().serializeNulls().create();
    private static final String XSD_STRING = "http://www.w3.org/2001/XMLSchema#string";

    private static Node term(JsonArray row, int offset) {
        String value = row.get(offset).getAsString();
        return switch (row.get(offset + 1).getAsString()) {
            case "IRI" -> NodeFactory.createURI(value);
            case "BLANK" -> NodeFactory.createBlankNode(value);
            default -> !row.get(offset + 3).isJsonNull()
                ? NodeFactory.createLiteralLang(value, row.get(offset + 3).getAsString())
                : NodeFactory.createLiteralDT(value, TypeMapper.getInstance().getSafeTypeByName(
                    row.get(offset + 2).isJsonNull() ? XSD_STRING : row.get(offset + 2).getAsString()));
        };
    }

    private static JsonElement term(Node node) {
        if (node == null) return JsonNull.INSTANCE;
        JsonObject value = new JsonObject();
        if (node.isURI()) {
            value.addProperty("type", "uri"); value.addProperty("value", node.getURI());
        } else if (node.isBlank()) {
            value.addProperty("type", "bnode"); value.addProperty("value", node.getBlankNodeLabel());
        } else {
            value.addProperty("type", "literal"); value.addProperty("value", node.getLiteralLexicalForm());
            value.addProperty("datatype", node.getLiteralDatatypeURI());
            value.add("lang", node.getLiteralLanguage().isEmpty() ? JsonNull.INSTANCE : new JsonPrimitive(node.getLiteralLanguage().toLowerCase(Locale.ROOT)));
        }
        return value;
    }

    private static JsonObject execute(Dataset dataset, JsonObject request) {
        String text = request.get("query").getAsString();
        String base = request.has("base") ? request.get("base").getAsString() : null;
        boolean update = request.has("update") && request.get("update").getAsBoolean();
        if (request.get("op").getAsString().equals("sparql-syntax")) {
            if (update) UpdateFactory.create(text, base, Syntax.syntaxSPARQL_11);
            else QueryFactory.create(text, base, Syntax.syntaxSPARQL_11);
            JsonObject result = new JsonObject(); result.addProperty("parsed", true); return result;
        }
        dataset.begin(ReadWrite.WRITE);
        try {
            dataset.asDatasetGraph().clear();
            if (request.has("named_graphs")) for (JsonElement name : request.getAsJsonArray("named_graphs")) {
                dataset.addNamedModel(name.getAsString(), org.apache.jena.rdf.model.ModelFactory.createDefaultModel());
            }
            for (JsonElement raw : request.getAsJsonArray("quads")) {
                JsonArray row = raw.getAsJsonArray();
                Node graph = row.get(0).isJsonNull() ? Quad.defaultGraphNodeGenerated : NodeFactory.createURI(row.get(0).getAsString());
                dataset.asDatasetGraph().add(new Quad(graph, term(row, 1), term(row, 5), term(row, 9)));
            }
            JsonObject result = new JsonObject();
            if (update) {
                UpdateAction.execute(UpdateFactory.create(text, base, Syntax.syntaxSPARQL_11), dataset);
                JsonArray quads = new JsonArray();
                dataset.asDatasetGraph().find().forEachRemaining(q -> {
                    JsonArray row = new JsonArray(); row.add(q.isDefaultGraph() ? JsonNull.INSTANCE : term(q.getGraph()));
                    row.add(term(q.getSubject())); row.add(term(q.getPredicate())); row.add(term(q.getObject())); quads.add(row);
                });
                result.add("quads", quads);
                JsonArray names = new JsonArray(); dataset.listNames().forEachRemaining(names::add); result.add("named_graphs", names);
            } else {
                Query query = QueryFactory.create(text, base, Syntax.syntaxSPARQL_11);
                try (QueryExecution execution = QueryExecution.dataset(dataset).query(query).timeout(20, TimeUnit.SECONDS).build()) {
                    if (query.isAskType()) result.addProperty("boolean", execution.execAsk());
                    else if (query.isSelectType()) {
                        ResultSet rs = execution.execSelect(); List<String> variables = rs.getResultVars();
                        result.add("variables", JSON.toJsonTree(variables)); JsonArray rows = new JsonArray();
                        while (rs.hasNext()) {
                            QuerySolution solution = rs.nextSolution(); JsonArray row = new JsonArray();
                            for (String variable : variables) row.add(term(solution.contains(variable) ? solution.get(variable).asNode() : null));
                            rows.add(row);
                        }
                        result.add("rows", rows);
                    } else {
                        Iterator<Triple> triples = query.isConstructType() ? execution.execConstructTriples() : execution.execDescribeTriples();
                        JsonArray graph = new JsonArray();
                        triples.forEachRemaining(t -> { JsonArray row = new JsonArray(); row.add(term(t.getSubject())); row.add(term(t.getPredicate())); row.add(term(t.getObject())); graph.add(row); });
                        result.add("graph", graph);
                    }
                }
            }
            dataset.commit(); return result;
        } catch (RuntimeException failure) { dataset.abort(); throw failure; }
        finally { dataset.end(); }
    }

    public static void main(String[] args) throws Exception {
        Path directory = Files.createTempDirectory("crabgraph-conformance-jena-");
        Dataset dataset = TDB2Factory.connectDataset(directory.toString());
        try (BufferedReader input = new BufferedReader(new InputStreamReader(System.in))) {
            for (String line; (line = input.readLine()) != null;) {
                JsonObject result;
                try { result = execute(dataset, JsonParser.parseString(line).getAsJsonObject()); }
                catch (Exception error) { result = new JsonObject(); result.addProperty("error", error.toString()); }
                System.out.println(JSON.toJson(result));
            }
        } finally {
            dataset.close();
            try (var paths = Files.walk(directory)) { for (Path path : paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(path); }
        }
    }
}
