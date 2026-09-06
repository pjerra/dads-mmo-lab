# Spike: does a published port reach a listener bound to the container's own loopback?

**Question.** Phase 8's command channel rests on one unmeasured claim: that a listener bound to
`127.0.0.1` *inside* a container cannot be reached through a Docker published port, and that the
listener must therefore bind all interfaces inside the container while the host side is pinned to
loopback. All three Phase 8 designs asserted or assumed something about this and none had measured
it; the panel called it "the load-bearing unmeasured claim" of the phase.

**Why it could be answered without a server.** The question is about Docker's port publishing, not
about any emulator. So it is answered with a busybox stand-in — gate the tool, not the payload —
which needs no WoW server, no rebuild, and no access to the owner's live machine.

**Where and when.** `PK-M910q`, 2026-09-06T13:44Z, Docker 29.7.2, kernel 6.8.0-138-generic.
Announced on that box's activity terminal before and after. Two throwaway `busybox:latest`
containers were created and removed; **no server was started, stopped or reconfigured**, and the
Vanilla and Tortoise stacks on that box were not touched.

## Method

Three cases, one probe. Each container publishes container port 9999 on the host at
`127.0.0.1:<port>`; the only difference is what the process inside binds to.

| Case | Command inside the container |
|---|---|
| **lo** | `nc -l -p 9999 -s 127.0.0.1` — bind the container's own loopback |
| **all** | `nc -l -p 9999` — bind all interfaces |
| **control** | nothing listening on that host port at all |

The probe connects from the host and distinguishes three outcomes: the connection is refused;
the connection is accepted but no data arrives; data arrives.

```python
s = socket.socket(); s.settimeout(4)
try: s.connect(("127.0.0.1", PORT))
except Exception as e: print("REFUSED:" + type(e).__name__); sys.exit()
d = s.recv(100)
print("DATA:" + d.decode().strip() if d else "SILENT:empty")
```

## Result

```
[lo]  listener inside: 127.0.0.1:9999
[lo]  from host      : SILENT:empty
[all] listener inside: :::9999
[all] from host      : DATA:PAYLOAD-all
[control, nothing bound] from host: REFUSED:ConnectionRefusedError
```

## What it settles, and what it corrects

**The design's conclusion holds: the listener must bind all interfaces inside the container.** A
listener on the container's own loopback does not serve a published port.

**The design's stated reason was wrong, in the direction that matters.** The documents said a
published port "cannot reach" such a listener, which implies a refused connection. It does not.
Docker accepts the connection on the host side and then cannot relay it, so the client gets an
established socket and **silence**. The misconfiguration answers.

Three consequences, each of which changed a document:

1. **A connect probe cannot verify the channel.** It succeeds on the broken configuration. Only a
   round-trip that receives a reply distinguishes the three states, which is why 8.2a's definition
   of done requires the reply *text* in the capture rather than evidence that the port answered.
2. **The three measured outcomes are the three the seam already types.** Refused is "nothing is
   listening" — a no. Silent is **could-not-ask**, not a no and not a yes. Data is a yes. The
   three-outcome rule turns out to be the shape the transport itself has.
3. **"Connected and silent" is also what a still-loading server looks like** to the same probe, so
   the seam cannot tell a misconfigured bind from a world that has not finished loading by the
   connection alone. What separates them is the container's ready marker, which is why the
   observability step comes first and why the enable press requires the world stopped: after the
   user's own Start, a silent port on a server whose ready marker *is* in the log is a
   misconfiguration, and the app can say so instead of waiting forever.

## What it does not settle

That the emulators behave like `nc`. They bind with the same system call and the reads record their
default as `127.0.0.1` inside the container, so the mechanism is the same; but no Yu'lon install has
run this listener, and the first per-family gate is still where that is proved. This spike removes
the *networking* question, not the per-tree one.
