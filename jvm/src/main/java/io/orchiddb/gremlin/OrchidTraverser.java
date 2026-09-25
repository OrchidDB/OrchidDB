package io.orchiddb.gremlin;

import java.lang.reflect.Proxy;
import java.util.*;
import org.apache.tinkerpop.gremlin.process.traversal.Traverser;
import org.apache.tinkerpop.gremlin.process.traversal.Pop;
import org.apache.tinkerpop.gremlin.process.traversal.Path;
import org.apache.tinkerpop.gremlin.process.traversal.step.util.MutablePath;

/** Read-only callback view of a traverser owned by the relational executor. */
final class OrchidTraverser {
    private OrchidTraverser() { }
    @SuppressWarnings("unchecked")
    static Traverser<?> create(Map<String,Object> frame, OrchidGraph graph) {
        Map<String,Object> state=new LinkedHashMap<>();
        ((Map<String,Object>)frame.get("bindings")).forEach((k,v)->state.put(k,OrchidCodec.decode(v,graph)));
        Path path=MutablePath.make();
        Object history=state.get("__path");
        List<Object> items=history instanceof Path p?p.objects():Collections.singletonList(state.get("current"));
        List<?> labels=state.get("__path_labels") instanceof List<?> l?l:List.of();
        for(int i=0;i<items.size();i++) {
            Set<String> names=new LinkedHashSet<>();
            if(i<labels.size()&&labels.get(i) instanceof Collection<?> c) for(Object label:c) names.add((String)label);
            path.extend(items.get(i),names);
        }
        return (Traverser<?>)Proxy.newProxyInstance(OrchidTraverser.class.getClassLoader(),new Class<?>[]{Traverser.class},(proxy,method,args)->{
            switch(method.getName()) {
                case "get": return state.get("current");
                case "bulk": return ((Number)frame.get("bulk")).longValue();
                case "loops": return ((Number)state.getOrDefault(args==null||args.length==0?"__loops":"__loops:"+args[0],0)).intValue();
                case "sack": if(args==null||args.length==0)return state.get("__sack");break;
                case "path":
                    if(args==null||args.length==0)return path;
                    if(args.length==1)return path.get((String)args[0]);
                    return path.get((Pop)args[0],(String)args[1]);
                case "toString": return String.valueOf(state.get("current"));
                case "hashCode": return System.identityHashCode(proxy);
                case "equals": return proxy==args[0];
            }
            throw new UnsupportedOperationException("Callback cannot mutate executor-owned traverser state: "+method.getName());
        });
    }
}
