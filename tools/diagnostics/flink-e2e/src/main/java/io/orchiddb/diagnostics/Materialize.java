package io.orchiddb.diagnostics;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.apache.flink.api.common.eventtime.WatermarkStrategy;
import org.apache.flink.api.common.functions.OpenContext;
import org.apache.flink.api.common.serialization.SimpleStringSchema;
import org.apache.flink.api.common.state.ValueState;
import org.apache.flink.api.common.state.ValueStateDescriptor;
import org.apache.flink.connector.base.DeliveryGuarantee;
import org.apache.flink.connector.kafka.source.KafkaSource;
import org.apache.flink.connector.kafka.source.enumerator.initializer.OffsetsInitializer;
import org.apache.flink.connector.kafka.sink.KafkaSink;
import org.apache.flink.connector.kafka.sink.KafkaRecordSerializationSchema;
import org.apache.flink.streaming.api.environment.StreamExecutionEnvironment;
import org.apache.flink.streaming.api.functions.KeyedProcessFunction;
import org.apache.flink.util.Collector;
import org.apache.kafka.clients.producer.ProducerRecord;
import java.nio.charset.StandardCharsets;
import java.util.List;

/** Real stateful materialization: per-key sum, duplicate suppression and versioned deletes. */
public final class Materialize {
    private static final ObjectMapper JSON = new ObjectMapper();
    public static final class Sum extends KeyedProcessFunction<String,String,String> {
        private transient ValueState<Long> total;
        private transient ValueState<Long> version;
        private transient ValueState<String> previous;
        @Override public void open(OpenContext ignored) {
            total = getRuntimeContext().getState(new ValueStateDescriptor<>("total", Long.class));
            version = getRuntimeContext().getState(new ValueStateDescriptor<>("version", Long.class));
            previous = getRuntimeContext().getState(new ValueStateDescriptor<>("previous", String.class));
        }
        @Override public void processElement(String value, Context context, Collector<String> out) throws Exception {
            JsonNode input = JSON.readTree(value);
            long next = input.path("version").asLong();
            Long old = version.value();
            if (old != null && next <= old) {
                if (next == old && !JSON.readTree(previous.value()).equals(input)) throw new IllegalStateException("conflicting version");
                return;
            }
            ObjectNode result = JSON.createObjectNode();
            result.put("table", "totals"); result.put("key", input.path("key").asText()); result.put("version", next);
            if (input.get("data").isNull()) { total.clear(); result.putNull("data"); }
            else {
                long sum = Math.addExact(total.value() == null ? 0L : total.value(), input.path("data").path("delta").asLong());
                total.update(sum); result.putObject("data").put("total",sum);
            }
            version.update(next); previous.update(value); out.collect(JSON.writeValueAsString(result));
        }
    }
    public static void main(String[] args) throws Exception {
        String input = args[0], output = args[1];
        StreamExecutionEnvironment env = StreamExecutionEnvironment.getExecutionEnvironment();
        env.setParallelism(1); env.enableCheckpointing(500);
        KafkaSource<String> source = KafkaSource.<String>builder().setBootstrapServers("kafka:29092")
            .setTopics(input).setGroupId("orchid-e2e-"+input).setStartingOffsets(OffsetsInitializer.earliest())
            .setValueOnlyDeserializer(new SimpleStringSchema()).build();
        KafkaRecordSerializationSchema<String> serializer = (value, context, timestamp) -> {
            try {
                JsonNode row = JSON.readTree(value);
                byte[] key = JSON.writeValueAsBytes(List.of("totals",row.path("key").asText()));
                return new ProducerRecord<>(output, key, value.getBytes(StandardCharsets.UTF_8));
            } catch (Exception e) { throw new RuntimeException(e); }
        };
        KafkaSink<String> sink = KafkaSink.<String>builder().setBootstrapServers("kafka:29092")
            .setRecordSerializer(serializer).setDeliveryGuarantee(DeliveryGuarantee.EXACTLY_ONCE)
            .setTransactionalIdPrefix("orchid-e2e-"+output).setProperty("transaction.timeout.ms","60000").build();
        env.fromSource(source, WatermarkStrategy.noWatermarks(), "input")
            .keyBy(value -> JSON.readTree(value).path("key").asText()).process(new Sum()).name("materialized-sums")
            .sinkTo(sink).name("versioned-output");
        env.execute("Orchid Kafka materialization E2E");
    }
}
