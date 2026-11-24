"""Messages exchanged with the Segno core: JSON Lines, one JSON object per line.

The core starts the worker and writes requests to its stdin; the worker answers on stdout.
In the cloud, the same messages will travel over a queue. Job messages
(`job.start`, `job.progress`, `job.result`) arrive in Phase 2, defined in `schemas/`.
"""

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter

PROTOCOL_VERSION = 1


class Message(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)


# Core → engine


class Ping(Message):
    """Liveness check; answered with a `pong` carrying the same id."""

    type: Literal["ping"] = "ping"
    id: str | None = None


class Shutdown(Message):
    """Asks the worker to exit cleanly."""

    type: Literal["shutdown"] = "shutdown"


type Request = Annotated[Ping | Shutdown, Field(discriminator="type")]
REQUESTS: TypeAdapter[Request] = TypeAdapter(Request)


# Engine → core


class Hello(Message):
    """First message of every worker: who is answering and which protocol it speaks."""

    type: Literal["hello"] = "hello"
    engine_version: str
    protocol_version: int = PROTOCOL_VERSION


class Pong(Message):
    type: Literal["pong"] = "pong"
    id: str | None = None


class ProtocolError(Message):
    """A request the worker could not understand. The worker keeps serving."""

    type: Literal["error"] = "error"
    message: str
