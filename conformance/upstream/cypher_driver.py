"""Lossless driver-value formatting in the TCK's temporal notation."""
from neo4j.time import Date, Time, DateTime, Duration

def temporal(value):
    if isinstance(value, (Date, Duration)) and not isinstance(value, DateTime):
        return value.iso_format()
    if not isinstance(value, (Time, DateTime)):
        raise ValueError('Unsupported temporal driver value ' + type(value).__name__)
    result = f'{value.hour:02}:{value.minute:02}'
    if value.second or value.nanosecond:
        result += f':{value.second:02}'
    if value.nanosecond:
        result += f'.{value.nanosecond:09}'.rstrip('0')
    if isinstance(value, DateTime):
        result = value.date().iso_format() + 'T' + result
    offset = value.utcoffset()
    if offset is not None:
        seconds = int(offset.total_seconds())
        magnitude = abs(seconds)
        if seconds == 0:
            result += 'Z'
        else:
            result += ('-' if seconds < 0 else '+') + f'{magnitude // 3600:02}:{magnitude % 3600 // 60:02}'
            if magnitude % 60:
                result += f':{magnitude % 60:02}'
        zone = getattr(value.tzinfo, 'zone', None) or getattr(value.tzinfo, 'key', None)
        if isinstance(value, DateTime) and zone and zone not in ('UTC', 'GMT', 'Z'):
            result += '[' + zone + ']'
    return result
