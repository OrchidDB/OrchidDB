package io.orchiddb.trino;

import io.airlift.slice.Slice;
import io.airlift.slice.Slices;
import io.trino.spi.block.*;
import io.trino.spi.type.*;
import org.apache.arrow.vector.*;
import org.apache.arrow.vector.complex.*;
import org.apache.arrow.vector.dictionary.DictionaryProvider;
import org.apache.arrow.vector.types.pojo.ArrowType;
import java.math.*;
import java.time.LocalTime;
import java.util.List;

/** Checked Arrow-to-Trino conversion. No string fallback for unsupported types. */
final class VectorToPage {
    private VectorToPage() {}
    static void write(Type type,FieldVector vector,int row,BlockBuilder output,DictionaryProvider dictionaries) {
        if(vector.isNull(row)){output.appendNull();return;}
        if(vector.getField().getDictionary()!=null) {
            var dictionary=dictionaries.lookup(vector.getField().getDictionary().getId());
            if(dictionary==null){throw new IllegalArgumentException("missing Arrow dictionary");}
            write(type,dictionary.getVector(),Math.toIntExact(((Number)vector.getObject(row)).longValue()),output,dictionaries);return;
        }
        if(type instanceof ArrayType array) {
            FieldVector values;int start,end;
            if(vector instanceof ListVector list){values=list.getDataVector();start=list.getElementStartIndex(row);end=list.getElementEndIndex(row);}
            else if(vector instanceof LargeListVector list){values=list.getDataVector();start=Math.toIntExact(list.getElementStartIndex(row));end=Math.toIntExact(list.getElementEndIndex(row));}
            else if(vector instanceof FixedSizeListVector list){values=list.getDataVector();start=list.getElementStartIndex(row);end=list.getElementEndIndex(row);}
            else{throw mismatch(type,vector);}
            ((ArrayBlockBuilder)output).buildEntry(elements->{for(int i=start;i<end;i++){write(array.getElementType(),values,i,elements,dictionaries);}});return;
        }
        if(type instanceof MapType map) {
            if(!(vector instanceof MapVector values)){throw mismatch(type,vector);}
            StructVector entries=(StructVector)values.getDataVector();
            List<FieldVector> fields=entries.getChildrenFromFields();
            ((MapBlockBuilder)output).buildEntry((keys,items)->{
                for(int i=values.getElementStartIndex(row);i<values.getElementEndIndex(row);i++) {
                    if(fields.get(0).isNull(i)){throw new IllegalArgumentException("null Arrow map key");}
                    write(map.getKeyType(),fields.get(0),i,keys,dictionaries);
                    write(map.getValueType(),fields.get(1),i,items,dictionaries);
                }
            });return;
        }
        if(type instanceof RowType record) {
            if(!(vector instanceof StructVector fields)){throw mismatch(type,vector);}
            List<FieldVector> children=fields.getChildrenFromFields();
            if(children.size()!=record.getTypeParameters().size()){throw mismatch(type,vector);}
            ((RowBlockBuilder)output).buildEntry(builders->{for(int i=0;i<children.size();i++){
                write(record.getTypeParameters().get(i),children.get(i),row,builders.get(i),dictionaries);
            }});return;
        }
        Object value=vector.getObject(row);
        if(type instanceof DecimalType decimal) {
            BigDecimal number=value instanceof BigDecimal d?d:new BigDecimal(value.toString());
            BigInteger unscaled=number.setScale(decimal.getScale(),RoundingMode.UNNECESSARY).unscaledValue();
            if(unscaled.abs().toString().length()>decimal.getPrecision()){throw new IllegalArgumentException("Arrow decimal exceeds Trino precision");}
            if(decimal.isShort()){type.writeLong(output,unscaled.longValueExact());}
            else{type.writeObject(output,Int128.valueOf(unscaled));}return;
        }
        if(type instanceof TimestampType timestamp || type instanceof TimestampWithTimeZoneType) {
            if(!(vector instanceof TimeStampVector values)){throw mismatch(type,vector);}
            ArrowType.Timestamp arrow=(ArrowType.Timestamp)vector.getField().getType();
            long raw=values.get(row),micros;int picosOfMicro;
            switch(arrow.getUnit()) {
                case SECOND -> {micros=Math.multiplyExact(raw,1_000_000L);picosOfMicro=0;}
                case MILLISECOND -> {micros=Math.multiplyExact(raw,1000L);picosOfMicro=0;}
                case MICROSECOND -> {micros=raw;picosOfMicro=0;}
                case NANOSECOND -> {micros=Math.floorDiv(raw,1000);picosOfMicro=(int)Math.floorMod(raw,1000)*1000;}
                default -> throw mismatch(type,vector);
            }
            if(type instanceof TimestampType timestamp) {
                if(timestamp.isShort()){
                    if(picosOfMicro!=0){throw new IllegalArgumentException("timestamp precision loss");}type.writeLong(output,micros);
                }else{type.writeObject(output,new LongTimestamp(micros,picosOfMicro));}
            }else{
                TimestampWithTimeZoneType timestamp=(TimestampWithTimeZoneType)type;
                long millis=Math.floorDiv(micros,1000);
                int picosOfMilli=(int)Math.floorMod(micros,1000)*1_000_000+picosOfMicro;
                if(timestamp.isShort()){
                    if(picosOfMilli!=0){throw new IllegalArgumentException("zoned timestamp precision loss");}
                    type.writeLong(output,DateTimeEncoding.packDateTimeWithZone(millis,TimeZoneKey.UTC_KEY));
                }else{type.writeObject(output,LongTimestampWithTimeZone.fromEpochMillisAndFraction(millis,picosOfMilli,TimeZoneKey.UTC_KEY));}
            }return;
        }
        if(type instanceof DateType) {
            if(vector instanceof DateDayVector date){type.writeLong(output,date.get(row));return;}
            if(vector instanceof DateMilliVector date){
                long millis=date.get(row);if(Math.floorMod(millis,86_400_000)!=0){throw new IllegalArgumentException("date precision loss");}
                type.writeLong(output,Math.floorDiv(millis,86_400_000));return;
            }throw mismatch(type,vector);
        }
        if(type instanceof TimeType) {
            long picos;
            if(value instanceof LocalTime time){picos=Math.multiplyExact(time.toNanoOfDay(),1000);}
            else if(value instanceof Number number && vector.getField().getType() instanceof ArrowType.Time arrow){
                long multiplier=switch(arrow.getUnit()){case SECOND->1_000_000_000_000L;case MILLISECOND->1_000_000_000L;case MICROSECOND->1_000_000L;case NANOSECOND->1000L;};
                picos=Math.multiplyExact(number.longValue(),multiplier);
            }else{throw mismatch(type,vector);}
            if(picos<0 || picos>=86_400_000_000_000_000L){throw new IllegalArgumentException("time outside one day");}
            type.writeLong(output,picos);return;
        }
        if(type instanceof BooleanType) {
            if(!(value instanceof Boolean bool)){throw mismatch(type,vector);}type.writeBoolean(output,bool);return;
        }
        if(type instanceof RealType) {
            if(!(value instanceof Number number)){throw mismatch(type,vector);}
            type.writeLong(output,Float.floatToRawIntBits(number.floatValue()));return;
        }
        if(type instanceof DoubleType) {
            if(!(value instanceof Number number)){throw mismatch(type,vector);}type.writeDouble(output,number.doubleValue());return;
        }
        if(type instanceof BigintType || type instanceof IntegerType || type instanceof SmallintType || type instanceof TinyintType) {
            if(!(value instanceof Number number)){throw mismatch(type,vector);}
            long integer=number instanceof BigInteger big?big.longValueExact():number.longValue();
            if(type instanceof IntegerType){Math.toIntExact(integer);}
            if(type instanceof SmallintType&&(integer<Short.MIN_VALUE||integer>Short.MAX_VALUE)){throw new IllegalArgumentException("smallint overflow");}
            if(type instanceof TinyintType&&(integer<Byte.MIN_VALUE||integer>Byte.MAX_VALUE)){throw new IllegalArgumentException("tinyint overflow");}
            type.writeLong(output,integer);return;
        }
        if(type.getJavaType()==Slice.class) {
            if(value instanceof byte[] bytes){type.writeSlice(output,Slices.wrappedBuffer(bytes));return;}
            if(value instanceof org.apache.arrow.vector.util.Text || value instanceof String){type.writeSlice(output,Slices.utf8Slice(value.toString()));return;}
        }
        throw mismatch(type,vector);
    }
    private static IllegalArgumentException mismatch(Type type,FieldVector vector) {
        return new IllegalArgumentException("Arrow "+vector.getField().getType()+" cannot populate Trino "+type.getDisplayName());
    }
}
