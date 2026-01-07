// @ts-nocheck
import { onMount, For, batch } from "solid-js";
import { createStore, reconcile } from "solid-js/store";

// Types based on the backend Rust structs
interface Room {
  id: string;
  name: string;
}

interface DebateTeam {
  id: string;
  team_id: string;
  side: number;
  seq: number;
}

interface Team {
  id: string;
  name: string;
}

interface Debate {
  id: string;
  number: number;
  room_id: string | null;
}

interface DebateRepr {
  debate: Debate;
  teams_of_debate: DebateTeam[];
  teams: Record<string, Team>;
  room: { name: string } | null;
}

interface RoundDrawRepr {
  round: { name: string; seq: number };
  debates: DebateRepr[];
}

interface DrawUpdate {
  unallocated_rooms: Room[];
  rounds: RoundDrawRepr[];
}

declare global {
  interface Window {
    drawRoomAllocatorConfig: {
      tournamentId: string;
      roundIds: string[];
    };
  }
}

function DrawRoomAllocator() {
  const [store, setStore] = createStore<DrawUpdate>({
    unallocated_rooms: [],
    rounds: [],
  });

  // Currently dragged room
  let draggedRoom: Room | null = null;

  // Move room via API call to backend
  const moveRoom = (
    roomId: string,
    toDebateId: string | null
  ) => {
    const { tournamentId, roundIds } = window.drawRoomAllocatorConfig;
    const body = new URLSearchParams();
    body.append("room_id", roomId);
    if (toDebateId) {
      body.append("to_debate_id", toDebateId);
    } else {
      body.append("to_debate_id", "");
    }
    roundIds.forEach((id) => body.append("rounds", id));

    fetch(`/tournaments/${tournamentId}/rounds/draws/rooms/edit/move`, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: body.toString(),
    }).catch((err) => console.error("Failed to move room:", err));
  };

  // Drag event handlers
  const handleDragStart = (
    e: DragEvent,
    room: Room
  ) => {
    draggedRoom = room;
    e.dataTransfer!.effectAllowed = "move";
    e.dataTransfer!.setData("text/plain", room.id);
    (e.target as HTMLElement).classList.add("dragging");
  };

  const handleDragEnd = (e: DragEvent) => {
    draggedRoom = null;
    (e.target as HTMLElement).classList.remove("dragging");
  };

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.dataTransfer!.dropEffect = "move";
    (e.currentTarget as HTMLElement).classList.add("drag-over");
  };

  const handleDragLeave = (e: DragEvent) => {
    (e.currentTarget as HTMLElement).classList.remove("drag-over");
  };

  const handleDropUnallocated = (e: DragEvent) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.remove("drag-over");
    if (draggedRoom) {
      moveRoom(draggedRoom.id, null);
    }
  };

  const handleDropDebate = (
    e: DragEvent,
    debateId: string,
  ) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.remove("drag-over");
    if (!draggedRoom) return;

    moveRoom(draggedRoom.id, debateId);
  };

  onMount(() => {
    const { tournamentId, roundIds } = window.drawRoomAllocatorConfig;
    const wsProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${wsProtocol}//${window.location.host}/tournaments/${tournamentId}/rounds/draws/rooms/edit/ws?rounds=${roundIds.join(
      ","
    )}`;

    const ws = new WebSocket(wsUrl);

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as DrawUpdate;
        batch(() => {
          setStore("unallocated_rooms", reconcile(data.unallocated_rooms));
          setStore("rounds", reconcile(data.rounds));
        });
      } catch (err) {
        console.error("Failed to parse WebSocket message:", err);
      }
    };

    ws.onerror = (error) => console.error("WebSocket error:", error);
    ws.onclose = () => console.log("WebSocket connection closed");
  });

  return (
    <div class="draw-editor">
      <div class="draw-editor-header">
        <h4>Room Allocator</h4>
      </div>

      <div class="unallocated-section">
        <h6>Unallocated Rooms</h6>
        <div
          class="unallocated-judges-container"
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDropUnallocated}
        >
          <For each={store.unallocated_rooms}>
            {(room) => (
              <div
                class="judge-badge"
                draggable={true}
                onDragStart={(e) => handleDragStart(e, room)}
                onDragEnd={handleDragEnd}
              >
                {room.name}
              </div>
            )}
          </For>
          {store.unallocated_rooms.length === 0 && (
            <div class="drop-placeholder">Drop rooms here to unassign</div>
          )}
        </div>
      </div>

      <For each={store.rounds}>
        {(round) => (
          <div class="round-section">
            <h5>{round.round.name}</h5>
            <table class="table table-bordered">
              <thead>
                <tr>
                  <th style="width: 100px;">Room</th>
                  <th style="width: 60px;">#</th>
                  <th>Teams</th>
                </tr>
              </thead>
              <tbody>
                <For each={round.debates}>
                  {(debate) => {
                    return (
                      <tr>
                        <td>
                          <div
                            class="judge-drop-zone"
                            onDragOver={handleDragOver}
                            onDragLeave={handleDragLeave}
                            onDrop={(e) =>
                              handleDropDebate(e, debate.debate.id)
                            }
                          >
                            {debate.room?.name && <div
                                class="judge-badge chair-badge"
                                draggable={true}
                                onDragStart={(e) => handleDragStart(e, {id: debate.debate.room_id, name: debate.room.name})}
                                onDragEnd={handleDragEnd}
                            >
                                <span>{debate.room?.name}</span>
                                <button
                                class="remove-btn"
                                onClick={() => moveRoom(debate.debate.room_id, null)}
                                title="Remove room"
                                >
                                ×
                                </button>
                            </div>}
                            {!debate.room?.name && (
                              <div class="judge-drop-zone-placeholder">
                                Room
                              </div>
                            )}
                          </div>
                        </td>
                        <td>{debate.debate.number}</td>
                        <td>
                          {debate.teams_of_debate
                            .map((dt) => debate.teams[dt.team_id]?.name)
                            .filter(Boolean)
                            .join(" vs ")}
                        </td>
                      </tr>
                    );
                  }}
                </For>
              </tbody>
            </table>
          </div>
        )}
      </For>
    </div>
  );
}

export default DrawRoomAllocator;
