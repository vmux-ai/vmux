# Collaborative Room ECS Design

## Goal

Turn each agent conversation into a stable room model that supports remote mobile clients now and multiple humans, agents, and collaborative documents later.

## Ownership

- `vmux_service` is the room-event authority for remote clients.
- `vmux_remote` owns Bevy-free wire identifiers and event DTOs.
- `vmux_agent` projects active rooms into ECS for desktop behavior.
- `vmux_mobile` keeps an ordered room-event projection.

ECS is a runtime projection. It is not the canonical transcript store.

## Runtime model

```text
ChatRoom
├── RoomMember
├── CollaborativeDocument
└── MaterializedRoomEvent
```

Existing agent stack entities remain in the layout hierarchy. `RoomAgentBinding` links them to rooms by stable `RoomId` and `MemberId`. Bevy entity IDs never cross process or network boundaries.

Each existing Page or ACP session receives one implicit room. Its current `AgentMessages` window is materialized as child event entities with deterministic event IDs and sequence numbers. Room projections are removed when the source session disappears.

## Event contract

Every committed event carries:

- stable room, event, and actor IDs;
- a monotonic server sequence;
- creation time;
- optional reply and client-operation IDs;
- the existing chat message payload.

Remote snapshots contain ordered `RoomEvent` values and a `through_seq` cursor. Mobile-generated prompts and new-chat requests carry a `ClientOpId`; the desktop deduplicates retries before dispatching agent work.

## CRDT boundary

The transcript remains an append-only ordered event log. CRDTs apply only to concurrently editable documents such as drafts, notes, and plans.

CRDTs do not own:

- permissions or approval decisions;
- presence;
- tool execution;
- terminal input;
- agent run state;
- transcript ordering.

`CollaborativeDocument` and `CrdtChangeReceived` establish the ECS seam without selecting or shipping a CRDT library yet.

## Migration path

1. Project current one-user agent sessions into implicit rooms.
2. Persist canonical room events outside layout persistence.
3. Replace deterministic legacy projection IDs with stored event IDs.
4. Add relay-backed subscriptions and membership.
5. Add a CRDT engine behind the collaborative-document seam.
