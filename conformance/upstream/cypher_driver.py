"""Lossless driver-value formatting in the TCK's temporal notation."""
from neo4j.time import Date, Time, DateTime, Duration

def bolt_date(days):
    """Proleptic Gregorian epoch-day decoding, including Neo4j's wide years."""
    days += 719468
    era, day = divmod(days, 146097)
    year_in_era = (day - day // 1460 + day // 36524 - day // 146096) // 365
    year = year_in_era + era * 400
    day -= 365 * year_in_era + year_in_era // 4 - year_in_era // 100
    month = (5 * day + 2) // 153
    day -= (153 * month + 2) // 5
    month += 3 if month < 10 else -9
    year += month <= 2
    prefix = '-' if year < 0 else '+' if year > 9999 else ''
    return f'{prefix}{abs(year):04}-{month:02}-{day + 1:02}'

def install_lossless_temporal_hydration():
    """Retain Bolt offset seconds discarded by the driver's pytz hydrators.

    Bolt 5 UTC datetime seconds and nanoseconds are decoded independently of
    query text. zoneinfo retains historical second-resolution named offsets.
    Install before creating any driver connections; only the peer adapter uses
    these hooks. The adapter tests exercise the actual Bolt connection too.
    """
    from datetime import datetime, timedelta, timezone
    from zoneinfo import ZoneInfo
    from neo4j._codec.hydration.v1 import temporal as v1
    from neo4j._codec.hydration.v2 import temporal as v2

    def hydrate_time(nanoseconds, offset=None):
        seconds, nano = divmod(nanoseconds, 1_000_000_000)
        hours, seconds = divmod(seconds, 3600)
        minutes, seconds = divmod(seconds, 60)
        text = f'{hours:02}:{minutes:02}'
        if seconds or nano:
            text += f':{seconds:02}'
        if nano:
            text += f'.{nano:09}'.rstrip('0')
        if offset == 0:
            text += 'Z'
        elif offset is not None:
            magnitude = abs(offset)
            text += ('-' if offset < 0 else '+') + f'{magnitude // 3600:02}:{magnitude % 3600 // 60:02}'
            if magnitude % 60:
                text += f':{magnitude % 60:02}'
        return text

    def hydrate_datetime(seconds, nanoseconds, zone=None):
        if isinstance(zone, str):
            instant = datetime(1970, 1, 1, tzinfo=timezone.utc) + timedelta(seconds=seconds)
            instant = instant.astimezone(ZoneInfo(zone))
            clock = (instant.hour * 3600 + instant.minute * 60 + instant.second) * 1_000_000_000 + nanoseconds
            text = instant.date().isoformat() + 'T' + hydrate_time(clock, int(instant.utcoffset().total_seconds()))
            return text + ('[' + zone + ']' if zone not in ('UTC', 'GMT', 'Z') else '')
        days, local_seconds = divmod(seconds + (zone or 0), 86400)
        return bolt_date(days) + 'T' + hydrate_time(local_seconds * 1_000_000_000 + nanoseconds, zone)

    v1.hydrate_date = bolt_date
    v1.hydrate_time = hydrate_time
    v2.hydrate_datetime = hydrate_datetime

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
