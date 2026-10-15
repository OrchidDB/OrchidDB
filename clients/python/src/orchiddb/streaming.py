"""Kafka execution with caller-owned permission checks (including SpiceDB)."""
import threading
from ._runtime import _Runtime


class StreamConnection:
    """One Kafka reader/writer, with a mandatory authorizer(subject, checks).

    The callback returns one bool per check, in order, and owns any permission
    caching/expiry. Calls are synchronous. Results contain base64 Arrow IPC;
    subscription frames carry replacement rows and a resumable batch checkpoint.
    Use separate connections for concurrent operations. Callback failures close
    this connection, cancelling pending work without advancing its checkpoint.
    """
    def __init__(self, schema, *, kafka, topic, source, authorizer, library=None,
                 auth_timeout_ms=30000, limits=None):
        if not callable(authorizer):
            raise TypeError("authorizer callback required")
        self._runtime = _Runtime(library)
        self._authorizer = authorizer
        self._lock = threading.Lock()
        self._id = self._runtime.operation_command(dict(op="stream_open", schema=schema,
            kafka=kafka, topic=topic, source=source, auth_timeout_ms=auth_timeout_ms, limits=limits))["id"]

    def _command(self, op, **values):
        if self._id is None:
            raise RuntimeError("Stream connection is closed")
        return self._runtime.operation_command(dict(op=op, id=self._id, **values))

    def _run(self, action, **values):
        if not self._lock.acquire(blocking=False):
            raise RuntimeError("Stream connection is busy")
        try:
            if hasattr(values.get("subject"), "to_dict"):
                values["subject"] = values["subject"].to_dict()
            self._command("stream_start", action=action, **values)
            while True:
                event = self._command("stream_poll")
                if event["state"] == "done":
                    return event["result"]
                if event["state"] == "authorize":
                    try:
                        decisions = self._authorizer(event["subject"], event["checks"])
                        if not isinstance(decisions, (list, tuple)) or len(decisions) != len(event["checks"]) or any(type(d) is not bool for d in decisions):
                            raise ValueError("authorizer must return one bool per check")
                        self._command("stream_authorize", nonce=event["nonce"], decisions=decisions)
                    except BaseException:
                        self.close()
                        raise
        finally:
            self._lock.release()

    def insert(self, change, *, subject):
        return self._run("insert", change=change, subject=subject)

    def insert_batch(self, changes, *, subject):
        return self._run("insert_batch", changes=changes, subject=subject)

    def query(self, text, *, subject, parameters=None, snapshot=None):
        return self._run("hybrid" if snapshot is not None else "query", text=text,
            subject=subject, parameters=parameters or {}, **({"snapshot": snapshot} if snapshot is not None else {}))

    def subscribe(self, text, *, subject, offsets, parameters=None):
        self._run("subscribe", text=text, subject=subject, offsets=offsets, parameters=parameters or {})
        return self

    def next_batch(self, max_records=1, wait_ms=100):
        return self._run("next", max_records=max_records, wait_ms=wait_ms)

    def unsubscribe(self):
        self._run("unsubscribe")

    def close(self):
        if self._id is not None:
            self._command("stream_close")
            self._id = None

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
