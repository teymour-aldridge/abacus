// @ts-nocheck
import { onMount, For, createSignal, Show, batch } from "solid-js";
import { createStore, reconcile } from "solid-js/store";

// Types based on the backend Rust structs
interface Judge {
  id: string;
  name: string;
  number: number;
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
  number: number;
}

interface Room {
  id: string;
  name: string;
}

interface Debate {
  id: string;
  number: number;
  room_id: string | null;
}

interface DebateJudge {
  id: string;
  judge_id: string;
  debate_id: string;
  status: string;
}

interface DebateRepr {
  debate: Debate;
  teams_of_debate: DebateTeam[];
  teams: Record<string, Team>;
  judges_of_debate: DebateJudge[];
  judges: Record<string, Judge>;
  room: { name: string } | null;
}

interface RoundDrawRepr {
  round: { name: string; seq: number };
  debates: DebateRepr[];
}

interface DrawUpdate {
  unallocated_judges: Judge[];
  unallocated_teams: Team[];
  unallocated_rooms: Room[];
  rounds: RoundDrawRepr[];
}

declare global {
  interface Window {
    drawEditorConfig: {
      tournamentId: string;
      roundIds: string[];
    };
  }
}

function DrawEditor() {
  const [store, setStore] = createStore<DrawUpdate>({
    unallocated_judges: [],
    unallocated_teams: [],
    unallocated_rooms: [],
    rounds: [],
  });

  // Edit mode: "judges" (default), "teams", or "rooms"
  const [editMode, setEditMode] = createSignal<"judges" | "teams" | "rooms">("judges");

  // Currently dragged judge
  let draggedJudge: { id: string; name: string; number: number } | null = null;

  // Currently dragged team and its source
  let draggedTeam: { id: string; name: string; debateId: string; side: number } | null = null;

  // Currently dragged room
  let draggedRoom: Room | null = null;

  // Get tournament and round info from window config
  const getTournamentId = () => window.drawEditorConfig?.tournamentId || "";
  const getRoundIds = () => window.drawEditorConfig?.roundIds || [];

  // Move judge via API call to backend
  const moveJudge = async (
    judgeId: string,
    toDebateId: string | null,
    role: string,
  ) => {
    const tournamentId = getTournamentId();
    const roundIds = getRoundIds();

    const formData = new FormData();
    formData.append("judge_id", judgeId);
    formData.append("to_debate_id", toDebateId || "");
    formData.append("role", role);
    roundIds.forEach((id) => formData.append("rounds", id));

    await fetch(`/tournaments/${tournamentId}/rounds/draws/edit/move`, {
      method: "POST",
      body: formData,
    });
  };

  // Swap teams via API call to backend
  const swapTeams = async (team1Id: string, team2Id: string) => {
    const tournamentId = getTournamentId();
    const roundIds = getRoundIds();

    const formData = new FormData();
    formData.append("team1_id", team1Id);
    formData.append("team2_id", team2Id);
    roundIds.forEach((id) => formData.append("rounds", id));

    await fetch(`/tournaments/${tournamentId}/rounds/draws/edit/move_team`, {
      method: "POST",
      body: formData,
    });
  };

  // Move room via API call to backend
  const moveRoom = async (roomId: string, toDebateId: string | null) => {
    const tournamentId = getTournamentId();
    const roundIds = getRoundIds();

    const body = new URLSearchParams();
    body.append("room_id", roomId);
    body.append("to_debate_id", toDebateId || "");
    roundIds.forEach((id) => body.append("rounds", id));

    await fetch(`/tournaments/${tournamentId}/rounds/draws/rooms/edit/move`, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: body.toString(),
    });
  };

  // Helper to get judges for a specific role in a debate
  const getJudgesForRole = (debate: DebateRepr, role: string): Judge[] => {
    return debate.judges_of_debate
      .filter((dj) => dj.status === role)
      .map((dj) => debate.judges[dj.judge_id])
      .filter(Boolean);
  };

  // Judge drag handlers
  const handleJudgeDragStart = (
    e: DragEvent,
    judge: { id: string; name: string; number: number },
  ) => {
    draggedJudge = judge;
    (e.target as HTMLElement).classList.add("dragging");
    e.dataTransfer!.effectAllowed = "move";
    e.dataTransfer!.setData("text/plain", judge.id);
    e.dataTransfer!.setData("application/x-judge", JSON.stringify(judge));
  };

  const handleJudgeDragEnd = (e: DragEvent) => {
    draggedJudge = null;
    (e.target as HTMLElement).classList.remove("dragging");
  };

  // Team drag handlers
  const handleTeamDragStart = (
    e: DragEvent,
    team: Team,
    debateId: string,
    side: number,
  ) => {
    draggedTeam = { id: team.id, name: team.name, debateId, side };
    (e.target as HTMLElement).classList.add("dragging");
    e.dataTransfer!.effectAllowed = "move";
    e.dataTransfer!.setData("text/plain", team.id);
    e.dataTransfer!.setData("application/x-team", JSON.stringify(draggedTeam));
  };

  const handleTeamDragEnd = (e: DragEvent) => {
    draggedTeam = null;
    (e.target as HTMLElement).classList.remove("dragging");
  };

  // Room drag handlers
  const handleRoomDragStart = (e: DragEvent, room: Room) => {
    draggedRoom = room;
    (e.target as HTMLElement).classList.add("dragging");
    e.dataTransfer!.effectAllowed = "move";
    e.dataTransfer!.setData("text/plain", room.id);
  };

  const handleRoomDragEnd = (e: DragEvent) => {
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

  // Handle dropping a judge on a debate
  const handleDropJudge = (
    e: DragEvent,
    toDebateId: string,
    role: string,
    existingJudges: Judge[],
  ) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.remove("drag-over");

    if (!draggedJudge) return;

    // For chair, only allow one
    if (role === "C" && existingJudges.length >= 1) {
      return;
    }

    moveJudge(draggedJudge.id, toDebateId, role);
  };

  // Handle dropping a judge on the unallocated area
  const handleDropUnallocated = (e: DragEvent) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.remove("drag-over");

    if (!draggedJudge) return;

    moveJudge(draggedJudge.id, null, "");
  };

  // Handle dropping a team on another team (swap)
  const handleDropTeam = (
    e: DragEvent,
    targetTeamId: string,
    targetDebateId: string,
  ) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.remove("drag-over");

    if (!draggedTeam || draggedTeam.id === targetTeamId) return;

    swapTeams(draggedTeam.id, targetTeamId);
  };

  // Handle dropping a room on a debate
  const handleDropRoom = (e: DragEvent, toDebateId: string) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.remove("drag-over");

    if (!draggedRoom) return;

    moveRoom(draggedRoom.id, toDebateId);
  };

  // Handle dropping a room on the unallocated area
  const handleDropUnallocatedRoom = (e: DragEvent) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.remove("drag-over");

    if (!draggedRoom) return;

    moveRoom(draggedRoom.id, null);
  };

  // Set up WebSocket connection
  onMount(() => {
    const tournamentId = getTournamentId();
    const roundIds = getRoundIds();

    if (!tournamentId || roundIds.length === 0) {
      console.error("Missing tournament config");
      return;
    }

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${protocol}//${window.location.host}/tournaments/${tournamentId}/rounds/draws/edit/ws?rounds=${roundIds.join(",")}`;

    const ws = new WebSocket(wsUrl);

    ws.onmessage = (event) => {
      try {
        const data: DrawUpdate = JSON.parse(event.data);
        batch(() => {
          setStore(reconcile(data));
        });
      } catch (err) {
        console.error("Failed to parse WebSocket message:", err);
      }
    };

    ws.onerror = (err) => {
      console.error("WebSocket error:", err);
    };

    ws.onclose = () => {
      console.log("WebSocket closed");
    };
  });

  return (
    <div class="draw-editor">
      <div class="draw-editor-header d-flex justify-content-between align-items-center">
        <h4>Draw Editor</h4>
        <div class="btn-group" role="group" aria-label="Edit mode toggle">
          <button
            type="button"
            class={`btn btn-sm ${editMode() === "judges" ? "btn-primary" : "btn-outline-primary"}`}
            onClick={() => setEditMode("judges")}
          >
            <i class="bi bi-person-badge me-1"></i>
            Edit Judges
          </button>
          <button
            type="button"
            class={`btn btn-sm ${editMode() === "teams" ? "btn-primary" : "btn-outline-primary"}`}
            onClick={() => setEditMode("teams")}
          >
            <i class="bi bi-people me-1"></i>
            Edit Teams
          </button>
          <button
            type="button"
            class={`btn btn-sm ${editMode() === "rooms" ? "btn-primary" : "btn-outline-primary"}`}
            onClick={() => setEditMode("rooms")}
          >
            <i class="bi bi-door-open me-1"></i>
            Edit Rooms
          </button>
        </div>
      </div>

      {/* Unallocated section - XOR between judges and teams */}
      <Show when={editMode() === "judges"}>
        <div class="unallocated-section">
          <h6>Unallocated Judges</h6>
          <div
            class="unallocated-judges-container"
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDropUnallocated}
          >
            <For each={store.unallocated_judges}>
              {(judge) => (
                <div
                  class="judge-badge"
                  draggable={true}
                  onDragStart={(e) => handleJudgeDragStart(e, judge)}
                  onDragEnd={handleJudgeDragEnd}
                >
                  <span class="badge-number">J{judge.number}</span>
                  <span>{judge.name}</span>
                </div>
              )}
            </For>
            {store.unallocated_judges.length === 0 && (
              <div class="drop-placeholder">
                All judges allocated. Drag judges here to unallocate.
              </div>
            )}
          </div>
        </div>
      </Show>

      <Show when={editMode() === "teams"}>
        <div class="unallocated-section">
          <h6>Team Editing Mode</h6>
          <div class="alert alert-info mb-3">
            <i class="bi bi-info-circle me-2"></i>
            Drag teams between positions to swap them. Teams can only be swapped with other teams already in the draw.
          </div>
        </div>
      </Show>

      <Show when={editMode() === "rooms"}>
        <div class="unallocated-section">
          <h6>Unallocated Rooms</h6>
          <div
            class="unallocated-judges-container"
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDropUnallocatedRoom}
          >
            <For each={store.unallocated_rooms}>
              {(room) => (
                <div
                  class="judge-badge"
                  draggable={true}
                  onDragStart={(e) => handleRoomDragStart(e, room)}
                  onDragEnd={handleRoomDragEnd}
                >
                  <i class="bi bi-door-open me-1"></i>
                  <span>{room.name}</span>
                </div>
              )}
            </For>
            {store.unallocated_rooms.length === 0 && (
              <div class="drop-placeholder">
                All rooms allocated. Drag rooms here to unallocate.
              </div>
            )}
          </div>
        </div>
      </Show>

      {/* Rounds and debates */}
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
                  <Show when={editMode() === "judges"}>
                    <th style="width: 180px;">Chair</th>
                    <th style="width: 200px;">Panelists</th>
                    <th style="width: 180px;">Trainees</th>
                  </Show>
                </tr>
              </thead>
              <tbody>
                <For each={round.debates}>
                  {(debate) => {
                    const chairs = () => getJudgesForRole(debate, "C");
                    const panelists = () => getJudgesForRole(debate, "P");
                    const trainees = () => getJudgesForRole(debate, "T");

                    // Get teams sorted by side
                    const sortedTeams = () =>
                      [...debate.teams_of_debate].sort((a, b) => {
                        if (a.side !== b.side) return a.side - b.side;
                        return a.seq - b.seq;
                      });

                    return (
                      <tr>
                        <td>
                          <Show
                            when={editMode() === "rooms"}
                            fallback={<span>{debate.room?.name || "TBA"}</span>}
                          >
                            <div
                              class="judge-drop-zone"
                              onDragOver={handleDragOver}
                              onDragLeave={handleDragLeave}
                              onDrop={(e) => handleDropRoom(e, debate.debate.id)}
                            >
                              {debate.room?.name ? (
                                <div
                                  class="judge-badge chair-badge"
                                  draggable={true}
                                  onDragStart={(e) => handleRoomDragStart(e, { id: debate.debate.room_id!, name: debate.room!.name })}
                                  onDragEnd={handleRoomDragEnd}
                                >
                                  <span>{debate.room.name}</span>
                                  <button
                                    class="remove-btn"
                                    onClick={() => moveRoom(debate.debate.room_id!, null)}
                                    title="Remove room"
                                  >
                                    ×
                                  </button>
                                </div>
                              ) : (
                                <div class="judge-drop-zone-placeholder">
                                  Room
                                </div>
                              )}
                            </div>
                          </Show>
                        </td>
                        <td>{debate.debate.number}</td>
                        <td>
                          <Show
                            when={editMode() === "teams"}
                            fallback={
                              <span>
                                {sortedTeams()
                                  .map((dt) => debate.teams[dt.team_id]?.name)
                                  .filter(Boolean)
                                  .join(" vs ")}
                              </span>
                            }
                          >
                            <div class="teams-edit-container">
                              <For each={sortedTeams()}>
                                {(dt, index) => {
                                  const team = () => debate.teams[dt.team_id];
                                  return (
                                    <>
                                      <div
                                        class={`team-badge side-${dt.side}`}
                                        draggable={true}
                                        onDragStart={(e) =>
                                          handleTeamDragStart(
                                            e,
                                            team(),
                                            debate.debate.id,
                                            dt.side,
                                          )
                                        }
                                        onDragEnd={handleTeamDragEnd}
                                        onDragOver={handleDragOver}
                                        onDragLeave={handleDragLeave}
                                        onDrop={(e) =>
                                          handleDropTeam(
                                            e,
                                            dt.team_id,
                                            debate.debate.id,
                                          )
                                        }
                                      >
                                        <span class="side-indicator">
                                          {getSideLabel(dt.side)}
                                        </span>
                                        <span>{team()?.name}</span>
                                      </div>
                                      {index() < sortedTeams().length - 1 && (
                                        <span class="vs-separator">vs</span>
                                      )}
                                    </>
                                  );
                                }}
                              </For>
                            </div>
                          </Show>
                        </td>
                        <Show when={editMode() === "judges"}>
                          <td>
                            <div
                              class="judge-drop-zone"
                              onDragOver={handleDragOver}
                              onDragLeave={handleDragLeave}
                              onDrop={(e) =>
                                handleDropJudge(
                                  e,
                                  debate.debate.id,
                                  "C",
                                  chairs(),
                                )
                              }
                            >
                              <For each={chairs()}>
                                {(judge) => (
                                  <div
                                    class="judge-badge chair-badge"
                                    draggable={true}
                                    onDragStart={(e) =>
                                      handleJudgeDragStart(e, judge)
                                    }
                                    onDragEnd={handleJudgeDragEnd}
                                  >
                                    <span>{judge.name}</span>
                                    <button
                                      class="remove-btn"
                                      onClick={() =>
                                        moveJudge(judge.id, null, "")
                                      }
                                      title="Remove judge"
                                    >
                                      ×
                                    </button>
                                  </div>
                                )}
                              </For>
                              {chairs().length === 0 && (
                                <div class="judge-drop-zone-placeholder">
                                  Chair (1)
                                </div>
                              )}
                            </div>
                          </td>
                          <td>
                            <div
                              class="judge-drop-zone"
                              onDragOver={handleDragOver}
                              onDragLeave={handleDragLeave}
                              onDrop={(e) =>
                                handleDropJudge(
                                  e,
                                  debate.debate.id,
                                  "P",
                                  panelists(),
                                )
                              }
                            >
                              <For each={panelists()}>
                                {(judge) => (
                                  <div
                                    class="judge-badge panel-badge"
                                    draggable={true}
                                    onDragStart={(e) =>
                                      handleJudgeDragStart(e, judge)
                                    }
                                    onDragEnd={handleJudgeDragEnd}
                                  >
                                    <span>{judge.name}</span>
                                    <button
                                      class="remove-btn"
                                      onClick={() =>
                                        moveJudge(judge.id, null, "")
                                      }
                                      title="Remove judge"
                                    >
                                      ×
                                    </button>
                                  </div>
                                )}
                              </For>
                              {panelists().length === 0 && (
                                <div class="judge-drop-zone-placeholder">
                                  Panelists
                                </div>
                              )}
                            </div>
                          </td>
                          <td>
                            <div
                              class="judge-drop-zone"
                              onDragOver={handleDragOver}
                              onDragLeave={handleDragLeave}
                              onDrop={(e) =>
                                handleDropJudge(
                                  e,
                                  debate.debate.id,
                                  "T",
                                  trainees(),
                                )
                              }
                            >
                              <For each={trainees()}>
                                {(judge) => (
                                  <div
                                    class="judge-badge trainee-badge"
                                    draggable={true}
                                    onDragStart={(e) =>
                                      handleJudgeDragStart(e, judge)
                                    }
                                    onDragEnd={handleJudgeDragEnd}
                                  >
                                    <span>{judge.name}</span>
                                    <button
                                      class="remove-btn"
                                      onClick={() =>
                                        moveJudge(judge.id, null, "")
                                      }
                                      title="Remove judge"
                                    >
                                      ×
                                    </button>
                                  </div>
                                )}
                              </For>
                              {trainees().length === 0 && (
                                <div class="judge-drop-zone-placeholder">
                                  Trainees
                                </div>
                              )}
                            </div>
                          </td>
                        </Show>
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

// Helper function to get side label (for BP: OG, OO, CG, CO; for 2-team: Prop, Opp)
function getSideLabel(side: number): string {
  // This assumes BP format. For 2-team debates, sides 0 and 1 are used
  const labels: Record<number, string> = {
    0: "OG",
    1: "OO",
    2: "CG",
    3: "CO",
  };
  return labels[side] ?? `S${side}`;
}

export default DrawEditor;
