package io.orchiddb.gremlin;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.util.*;
import java.util.stream.Collectors;
import org.apache.tinkerpop.gremlin.process.traversal.Bytecode;
import org.apache.tinkerpop.gremlin.process.traversal.Compare;
import org.apache.tinkerpop.gremlin.process.traversal.Contains;
import org.apache.tinkerpop.gremlin.process.traversal.Order;
import org.apache.tinkerpop.gremlin.process.traversal.P;
import org.apache.tinkerpop.gremlin.process.traversal.util.ConnectiveP;

/** Serializes a validated subset of structured bytecode, never a traversal's display string. */
final class BytecodeQuery {
  private static final ObjectMapper JSON = new ObjectMapper();

  private enum Shape {
    VERTEX,
    EDGE,
    SCALAR
  }

  static String translate(Bytecode code) {
    return traversal(code, null, false);
  }

  private static String traversal(Bytecode code, Shape input, boolean child) {
    if (!code.getSourceInstructions().isEmpty())
      throw unsupported("Traversal source options/strategies are not supported");
    if (code.getStepInstructions().isEmpty()) throw unsupported("Empty traversal");
    var out = new StringBuilder(child ? "__" : "g");
    Shape shape = input;
    String previous = "";
    for (var instruction : code.getStepInstructions()) {
      String op = instruction.getOperator();
      Object[] a = instruction.getArguments();
      if (shape == null && !op.equals("V") && !op.equals("E"))
        throw unsupported("Traversal must start with V() or E()");
      String args;
      switch (op) {
        case "V", "E" -> {
          if (shape != null || child) throw unsupported("Only initial V()/E() is supported");
          for (Object id : a)
            if (!(id instanceof Byte
                || id instanceof Short
                || id instanceof Integer
                || id instanceof Long))
              throw unsupported("Mapped graph IDs must be signed integers");
          shape = op.equals("V") ? Shape.VERTEX : Shape.EDGE;
          args = literals(a);
        }
        case "hasLabel", "hasNot" -> {
          element(shape, op);
          if (op.equals("hasNot")) arity(a, 1);
          else nonempty(a);
          strings(a);
          args = literals(a);
        }
        case "hasId" -> {
          element(shape, op);
          nonempty(a);
          args = arguments(a);
        }
        case "has" -> {
          element(shape, op);
          if (a.length < 1
              || a.length > 3
              || !(a[0] instanceof String)
              || (a.length == 3 && !(a[1] instanceof String)))
            throw unsupported("has() supports string keys and literal/P values");
          args = arguments(a);
        }
        case "out", "in", "both", "outE", "inE", "bothE" -> {
          if (shape != Shape.VERTEX) throw unsupported(op + " requires vertices");
          strings(a);
          args = literals(a);
          shape = op.endsWith("E") ? Shape.EDGE : Shape.VERTEX;
        }
        case "inV", "outV", "bothV" -> {
          if (shape != Shape.EDGE) throw unsupported(op + " requires edges");
          arity(a, 0);
          args = "";
          shape = Shape.VERTEX;
        }
        case "values" -> {
          element(shape, op);
          arity(a, 1);
          strings(a);
          args = literals(a);
          shape = Shape.SCALAR;
        }
        case "id", "label" -> {
          element(shape, op);
          arity(a, 0);
          args = "";
          shape = Shape.SCALAR;
        }
        case "count" -> {
          arity(a, 0);
          args = "";
          shape = Shape.SCALAR;
        }
        case "is" -> {
          if (shape != Shape.SCALAR) throw unsupported("is() requires scalar values");
          arity(a, 1);
          args = arguments(a);
        }
        case "dedup", "order", "identity" -> {
          arity(a, 0);
          args = "";
        }
        case "by" -> {
          if (!previous.equals("order")) throw unsupported("Only one order().by(...) is supported");
          if (a.length > 2) throw unsupported("Unsupported order modulator");
          var parts = new ArrayList<String>();
          for (Object v : a) {
            if (v == Order.asc || v == Order.desc) parts.add("Order." + ((Order) v).name());
            else if (v instanceof String && shape != Shape.SCALAR) parts.add(literal(v));
            else throw unsupported("by() supports a property key and/or Order.asc/desc");
          }
          args = String.join(",", parts);
        }
        case "limit", "skip", "range" -> {
          arity(a, op.equals("range") ? 2 : 1);
          for (Object v : a)
            if (!(v instanceof Long || v instanceof Integer) || ((Number) v).longValue() < 0)
              throw unsupported("Slice bounds must be nonnegative integers");
          args = literals(a);
        }
        case "filter", "not", "and", "or", "where" -> {
          if (op.equals("and") || op.equals("or")) nonempty(a);
          else arity(a, 1);
          var parts = new ArrayList<String>();
          for (Object v : a) {
            if (!(v instanceof Bytecode nested))
              throw unsupported(op + " requires anonymous traversals; lambdas are unsupported");
            parts.add(traversal(nested, shape, true));
          }
          args = String.join(",", parts);
        }
        default -> throw unsupported("Unsupported Gremlin step: " + op);
      }
      out.append('.').append(op).append('(').append(args).append(')');
      previous = op;
    }
    if (!child && shape != Shape.SCALAR)
      throw unsupported(
          "Project scalar results with values(key), id(), label(), or count(); vertex/edge/path objects are not supported yet");
    return out.toString();
  }

  private static String arguments(Object[] args) {
    return Arrays.stream(args)
        .map(v -> v instanceof P<?> p ? predicate(p) : literal(v))
        .collect(Collectors.joining(","));
  }

  private static String predicate(P<?> p) {
    if (p instanceof ConnectiveP<?> connective) {
      String name = p.getPredicateName();
      if (!name.equals("and") && !name.equals("or"))
        throw unsupported("Unsupported connective predicate");
      var predicates = connective.getPredicates();
      String result = predicate(predicates.get(0));
      for (int i = 1; i < predicates.size(); i++)
        result += "." + name + "(" + predicate(predicates.get(i)) + ")";
      return result;
    }
    String name = p.getPredicateName();
    if (!Set.of("eq", "neq", "lt", "lte", "gt", "gte", "within", "without").contains(name))
      throw unsupported("Unsupported predicate: " + name);
    if (!(p.getBiPredicate() instanceof Compare || p.getBiPredicate() instanceof Contains))
      throw unsupported("Custom predicate implementations are unsupported");
    Object value = p.getValue();
    String args;
    if (name.equals("within") || name.equals("without")) {
      if (!(value instanceof Collection<?> values))
        throw unsupported("Expected predicate collection");
      args = values.stream().map(BytecodeQuery::literal).collect(Collectors.joining(","));
    } else args = literal(value);
    return "P." + name + "(" + args + ")";
  }

  private static String literals(Object[] values) {
    return Arrays.stream(values).map(BytecodeQuery::literal).collect(Collectors.joining(","));
  }

  private static String literal(Object v) {
    if (v == null) return "null";
    if (v instanceof String s) {
      try {
        return JSON.writeValueAsString(s);
      } catch (JsonProcessingException e) {
        throw new IllegalArgumentException("Invalid string", e);
      }
    }
    if (v instanceof Boolean
        || v instanceof Byte
        || v instanceof Short
        || v instanceof Integer
        || v instanceof Long) return v.toString();
    if (v instanceof Float || v instanceof Double) {
      if (Double.isFinite(((Number) v).doubleValue())) return v.toString();
    }
    throw unsupported("Unsupported Gremlin argument type: " + v.getClass().getName());
  }

  private static void element(Shape shape, String op) {
    if (shape == Shape.SCALAR) throw unsupported(op + " requires vertices or edges");
  }

  private static void strings(Object[] a) {
    for (Object v : a) if (!(v instanceof String)) throw unsupported("Expected a string key/label");
  }

  private static void arity(Object[] a, int size) {
    if (a.length != size)
      throw unsupported("Unsupported step overload; expected " + size + " arguments");
  }

  private static void nonempty(Object[] a) {
    if (a.length == 0) throw unsupported("Expected at least one argument");
  }

  private static UnsupportedOperationException unsupported(String message) {
    return new UnsupportedOperationException(message);
  }
}
