// @ts-nocheck
import { onMount, For, batch } from "solid-js";
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
    rounds: [],
  });

  // Currently dragged judge
  let draggedJudge: { id: string; name: string; number: number } | null = null;

  // Move judge via API call to backend
  const moveJudge = (
    judgeId: string,
    toDebateId: string | null,
    role: string
  ) => {
    const { tournamentId, roundIds } = window.drawEditorConfig;
    const body = new URLSearchParams();
    body.append("judge_id", judgeId);
    if (toDebateId) {
      body.append("to_debate_id", toDebateId);
    } else {
      body.append("to_debate_id", "");
    }
    body.append("role", role);
    roundIds.forEach((id) => body.append("rounds", id));

    fetch(`/tournaments/${tournamentId}/rounds/draws/edit/move`, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: body.toString(),
    }).catch((err) => console.error("Failed to move judge:", err));
  };

  // Get judges for a specific role in a debate
  const getJudgesForRole = (debate: DebateRepr, status: string) => {
    return debate.judges_of_debate
      .filter((dj) => dj.status === status)
      .map((dj) => ({
        id: dj.judge_id,
        name: debate.judges[dj.judge_id]?.name || "Unknown",
        number: debate.judges[dj.judge_id]?.number || 0,
      }));
  };

  // Drag event handlers
  const handleDragStart = (
    e: DragEvent,
    judge: { id: string; name: string; number: number }
  ) => {
    draggedJudge = judge;
    e.dataTransfer!.effectAllowed = "move";
    e.dataTransfer!.setData("text/plain", judge.id);
    (e.target as HTMLElement).classList.add("dragging");
  };

  const handleDragEnd = (e: DragEvent) => {
    draggedJudge = null;
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
    if (draggedJudge) {
      moveJudge(draggedJudge.id, null, "");
    }
  };

  const handleDropDebate = (
    e: DragEvent,
    debateId: string,
    role: string,
    currentChairs: { id: string; name: string; number: number }[]
  ) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.remove("drag-over");
    if (!draggedJudge) return;

    moveJudge(draggedJudge.id, debateId, role);

    // If dropping a new chair and there was an existing chair, demote old one
    if (role === "C" && currentChairs.length > 0) {
      const oldChair = currentChairs.find((j) => j.id !== draggedJudge!.id);
      if (oldChair) {
        moveJudge(oldChair.id, debateId, "P");
      }
    }
  };

  onMount(() => {
    const { tournamentId, roundIds } = window.drawEditorConfig;
    const wsProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${wsProtocol}//${window.location.host}/tournaments/${tournamentId}/rounds/draws/edit/ws?rounds=${roundIds.join(
      ","
    )}`;

    const ws = new WebSocket(wsUrl);

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as DrawUpdate;
        batch(() => {
          setStore("unallocated_judges", reconcile(data.unallocated_judges));
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
        <h4>Draw Editor</h4>
      </div>

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
                onDragStart={(e) => handleDragStart(e, judge)}
                onDragEnd={handleDragEnd}
              >
                {judge.name} (j{judge.number})
              </div>
            )}
          </For>
          {store.unallocated_judges.length === 0 && (
            <div class="drop-placeholder">Drop judges here to unassign</div>
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
                  <th style="width: 180px;">Chair</th>
                  <th style="width: 200px;">Panelists</th>
                  <th style="width: 180px;">Trainees</th>
                </tr>
              </thead>
              <tbody>
                <For each={round.debates}>
                  {(debate) => {
                    const chairs = () => getJudgesForRole(debate, "C");
                    const panelists = () => getJudgesForRole(debate, "P");
                    const trainees = () => getJudgesForRole(debate, "T");

                    return (
                      <tr>
                        <td>{debate.room?.name || "TBA"}</td>
                        <td>{debate.debate.number}</td>
                        <td>
                          {debate.teams_of_debate
                            .map((dt) => debate.teams[dt.team_id]?.name)
                            .filter(Boolean)
                            .join(" vs ")}
                        </td>
                        <td>
                          <div
                            class="judge-drop-zone"
                            onDragOver={handleDragOver}
                            onDragLeave={handleDragLeave}
                            onDrop={(e) =>
                              handleDropDebate(e, debate.debate.id, "C", chairs())
                            }
                          >
                            <For each={chairs()}>
                              {(judge) => (
                                <div
                                  class="judge-badge chair-badge"
                                  draggable={true}
                                  onDragStart={(e) => handleDragStart(e, judge)}
                                  onDragEnd={handleDragEnd}
                                >
                                  <span>{judge.name}</span>
                                  <button
                                    class="remove-btn"
                                    onClick={() => moveJudge(judge.id, null, "")}
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
                              handleDropDebate(
                                e,
                                debate.debate.id,
                                "P",
                                panelists()
                              )
                            }
                          >
                            <For each={panelists()}>
                              {(judge) => (
                                <div
                                  class="judge-badge panel-badge"
                                  draggable={true}
                                  onDragStart={(e) => handleDragStart(e, judge)}
                                  onDragEnd={handleDragEnd}
                                >
                                  <span>{judge.name}</span>
                                  <button
                                    class="remove-btn"
                                    onClick={() => moveJudge(judge.id, null, "")}
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
                              handleDropDebate(
                                e,
                                debate.debate.id,
                                "T",
                                trainees()
                              )
                            }
                          >
                            <For each={trainees()}>
                              {(judge) => (
                                <div
                                  class="judge-badge trainee-badge"
                                  draggable={true}
                                  onDragStart={(e) => handleDragStart(e, judge)}
                                  onDragEnd={handleDragEnd}
                                >
                                  <span>{judge.name}</span>
                                  <button
                                    class="remove-btn"
                                    onClick={() => moveJudge(judge.id, null, "")}
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

export default DrawEditor;
