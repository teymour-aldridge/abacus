use crate::{
    state::Conn,
    tournaments::WEBSOCKET_SCHEME,
    util_resp::{StandardResponse, success},
    widgets::actions::Actions,
};
use diesel::prelude::*;
use hypertext::{Raw, prelude::*};
use itertools::Either;
use rocket::{
    State,
    futures::{SinkExt, StreamExt},
    get,
};
use tokio::{sync::broadcast::Receiver, task::spawn_blocking};

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::tournaments,
    state::DbPool,
    template::Page,
    tournaments::{Tournament, participants::TournamentParticipants},
};

pub mod create_speaker;
pub mod create_team;
pub mod manage_team;

#[get("/tournaments/<tid>/participants")]
pub async fn manage_tournament_participants(
    tid: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;

    let script = format!(
        r#"
        <script>
        const styleEl = document.createElement('style');
        styleEl.innerHTML = `
            .tabulator .tabulator-row {{
                transition: all 0.2s ease;
            }}
            .tabulator .tabulator-row:hover {{
                background-color: #f0e8f5 !important;
            }}
            .tabulator .tabulator-row.tabulator-row-even {{
                background-color: #fcfcfc;
            }}
            .tabulator .tabulator-row.tabulator-row-odd {{
                background-color: #f9f9f9;
            }}
            .tabulator .tabulator-cell {{
                padding: 12px 8px;
                border-right: none;
            }}
            .speakers-container {{
                box-shadow: 0 2px 5px rgba(69, 40, 89, 0.1);
                transition: all 0.3s ease;
            }}
            .speaker-heading {{
                display: flex;
                align-items: center;
            }}
            .speaker-heading::before {{
                content: "👥";
                margin-right: 8px;
            }}
            .custom-btn-primary {{
                background-color: #452859;
                border-color: #452859;
                color: white;
            }}
            .custom-btn-primary:hover {{
                background-color: #5a3873;
                border-color: #5a3873;
                color: white;
            }}
            .custom-btn-secondary {{
                background-color: white;
                border-color: #452859;
                color: #452859;
            }}
            .custom-btn-secondary:hover {{
                background-color: #f0e8f5;
                color: #452859;
            }}
            @media (max-width: 768px) {{
                .tabulator-cell[role="gridcell"] {{
                    white-space: normal;
                    word-break: break-word;
                }}
            }}
        `;
        document.head.appendChild(styleEl);

        var table = new Tabulator('#participants', {{
            layout: "fitColumns",
            responsiveLayout: "collapse",
            height: "100%",
            columnDefaults: {{
                resizable: true,
                headerSort: true,
                tooltip: true,
            }},
            data: [],
            rowFormatter: function(row) {{
                var holderEl = document.createElement("div");
                var tableEl = document.createElement("div");

                holderEl.style.boxSizing = "border-box";
                holderEl.classList.add("ms-4", "mt-3", "p-3", "mb-3", "speakers-container");
                holderEl.style.backgroundColor = "\#f8f9fa";
                holderEl.style.borderLeft = "4px solid #452859";
                holderEl.style.borderRadius = "0 8px 8px 0";

                let participants = document.createElement("h6");
                participants.textContent = "Speakers";
                participants.className = "text-muted mb-3 speaker-heading";
                participants.style.color = "\#452859";
                participants.style.fontWeight = "600";
                holderEl.appendChild(participants);

                tableEl.classList.add("mb-2");
                holderEl.appendChild(tableEl);

                row.getElement().appendChild(holderEl);

                var subTable = new Tabulator(tableEl, {{
                    layout: "fitColumns",
                    data: row.getData().speakers,
                    headerSort: false,
                    columns: [
                        {{title: "ID", field: "id", width: 80}},
                        {{title: "Name", field: "name", sorter: "string"}},
                        {{title: "Email", field: "email"}},
                    ],
                    initialSort: [
                        {{column: "name", dir: "asc"}}
                    ],
                    tableBuilt: function() {{
                        const headers = this.element.querySelectorAll('.tabulator-header');
                        headers.forEach(header => {{
                            header.style.backgroundColor = '#5a3873'; // Slightly lighter shade than main table
                            header.style.color = 'white';
                            header.style.fontWeight = '600';
                        }});

                        const headerCells = this.element.querySelectorAll('.tabulator-col-title');
                        headerCells.forEach(cell => {{
                            cell.style.color = 'white';
                        }});
                    }}
                }});
            }},
            columns: [
                {{title: "ID", field: "id", width: 80, headerTooltip: "Team ID"}},
                {{title: "Name", field: "name", headerTooltip: "Team Name"}},
                {{title: "Institution", field: "inst", headerTooltip: "Institution Name"}},
                {{title: "Actions", field: "actions", width: 220, headerSort: false, formatter: function(cell, formatterParams, onRendered) {{
                    let list = cell.getValue();

                    let a1 = document.createElement("a");
                    a1.className = "btn btn-sm custom-btn-secondary me-2";
                    a1.href = list[0];
                    a1.innerHTML = '<i class="fas fa-edit"></i> Edit';

                    let a2 = document.createElement("a");
                    a2.className = "btn btn-sm custom-btn-primary";
                    a2.href = list[1];
                    a2.innerHTML = '<i class="fas fa-user-plus"></i> Add speaker';

                    let div = document.createElement("div");
                    div.className = "d-flex flex-wrap gap-1";
                    div.appendChild(a1);
                    div.appendChild(a2);

                    return div;
                }} }}
            ],
            rowClick: function(e, row) {{
                const speakerContainer = row.getElement().querySelector('.speakers-container');
                if (speakerContainer) {{
                    if (speakerContainer.style.display === 'none') {{
                        speakerContainer.style.display = 'block';
                    }} else {{
                        speakerContainer.style.display = 'none';
                    }}
                }}
            }},
            tableBuilt: function() {{
                document.querySelector('#participants .tabulator').style.border = '1px solid #dee2e6';
                document.querySelector('#participants .tabulator').style.borderRadius = '8px';
                document.querySelector('#participants .tabulator').style.overflow = 'hidden';
                document.querySelector('#participants .tabulator').style.boxShadow = '0 4px 10px rgba(0,0,0,0.05)';

                let headers = document.querySelectorAll('#participants .tabulator-header');
                headers.forEach(header => {{
                    header.style.backgroundColor = '#452859';
                    header.style.color = 'white';
                    header.style.fontWeight = '600';
                    header.style.borderBottom = 'none';
                }});

                let headerCells = document.querySelectorAll('#participants .tabulator-col-title');
                headerCells.forEach(cell => {{
                    cell.style.color = 'white';
                }});

                if (!document.querySelector('link[href*="font-awesome"]')) {{
                    const fontAwesome = document.createElement('link');
                    fontAwesome.rel = 'stylesheet';
                    fontAwesome.href = 'https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css';
                    document.head.appendChild(fontAwesome);
                }}
            }}
        }});

        ws = new WebSocket(`{WEBSOCKET_SCHEME}${{window.location.host}}/tournaments/{tid}/participants?channel`);

        ws.onmessage = function(event) {{
            let data = JSON.parse(event.data);
            table.replaceData(data);

            document.querySelector('#participants').classList.remove('loading');
        }};

        ws.onopen = function() {{
            console.log("WebSocket connection established");
        }};

        ws.onerror = function(event) {{
            console.error("WebSocket error observed:", event);
            alert("WebSocket connection failed. Please refresh the page or try again later.");
        }};

        document.querySelector('#participants').classList.add('loading');
        document.querySelector('#participants').insertAdjacentHTML('beforeend', '<div class="text-center p-4 loading-indicator"><div class="spinner-border text-primary" role="status"><span class="visually-hidden">Loading...</span></div><p class="mt-2 text-muted">Loading participants...</p></div>');

        table.on("dataLoaded", function() {{
            const loadingIndicator = document.querySelector('.loading-indicator');
            if (loadingIndicator) loadingIndicator.remove();
        }});
        </script>
        "#,
    );

    success(
        Page::new()
            .user(user)
            .tournament(tournament)
            .body(maud! {
                script src="https://cdnjs.cloudflare.com/ajax/libs/tabulator/6.3.1/js/tabulator.min.js" integrity="sha512-8+qwMD/110YLl5T2bPupMbPMXlARhei2mSxerb/0UWZuvcg4NjG7FdxzuuvDs2rBr/KCNqhyBDe8W3ykKB1dzA==" crossorigin="anonymous" referrerpolicy="no-referrer" {}
                link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/tabulator/6.3.1/css/tabulator_bootstrap5.min.css" integrity="sha512-qDEgvDbdp7tq+ytU/OgCzWfvbfdEe3pv0yEOMz/gurMcR0BWNgIF6I4VKeoACEj5E5PFf1uo3Vzuwk/ga9zeUg==" crossorigin="anonymous" referrerpolicy="no-referrer";
                script type="text/javascript" src="https://unpkg.com/tabulator-tables@6.3.1/dist/js/tabulator.min.js" {}

                Actions options=(&[
                    (format!("/tournaments/{tid}/teams/create").as_str(), "Add team")
                ]);

                br;

                div #participants {}

                (Raw::dangerously_create(&script))
            })
            .render(),
    )
}

#[get("/tournaments/<tid>/participants?channel")]
pub async fn tournament_participant_updates(
    tid: &str,
    // todo: can replace with ThreadSafeConn where TX = false
    pool: &State<DbPool>,
    ws: rocket_ws::WebSocket,
    rx: &State<Receiver<Msg>>,
    user: User<false>,
) -> Option<rocket_ws::Channel<'static>> {
    let pool: DbPool = pool.inner().clone();

    let tid1 = tid.to_string();
    let pool1 = pool.clone();
    let tournament = spawn_blocking(move || {
        let mut conn = pool1.get().unwrap();
        let tournament = tournaments::table
            .filter(tournaments::id.eq(tid1))
            .first::<Tournament>(&mut conn)
            .optional()
            .unwrap();

        if let Some(tournament) = tournament {
            if tournament
                .check_user_has_permission(
                    &user.id,
                    crate::permission::Permission::ManageParticipants,
                    &mut *conn,
                )
                .is_err()
            {
                None
            } else {
                Some(tournament)
            }
        } else {
            None
        }
    })
    .await
    .unwrap();

    let tournament = match tournament {
        Some(t) => t,
        None => return None,
    };

    let pool2 = pool.clone();
    let tid2 = tid.to_string();
    let get_serializable_data = move || {
        let mut conn = pool2.get().unwrap();

        let p = conn
            .transaction(|conn| -> Result<_, diesel::result::Error> {
                Ok(TournamentParticipants::load(&tid2, &mut *conn)
                    .for_tabulator())
            })
            .unwrap();

        serde_json::to_string(&p).unwrap()
    };

    let mut rx: Receiver<Msg> = rx.inner().resubscribe();
    Some(ws.channel(move |mut stream| {
        Box::pin(async move {
            // we send this once at the start in order to populate the data in
            // the table. I believe this is faster than what Tabbycat does
            // (embedding all the data in the page)
            let serialized_data =
                spawn_blocking(get_serializable_data.clone()).await.unwrap();
            let _ =
                stream.send(rocket_ws::Message::Text(serialized_data)).await;

            // we then send the data again on each update
            loop {
                let msg = {
                    let msg = tokio::select! {
                        msg = rx.recv() => Either::Left(msg.unwrap()),
                        msg = stream.next() => Either::Right(msg.unwrap()),
                    };
                    let msg = match msg {
                        Either::Left(left) => left,
                        Either::Right(close) => {
                            let close = close?;
                            if matches!(close, rocket_ws::Message::Close(_)) {
                                return Ok(());
                            } else {
                                continue;
                            }
                        }
                    };

                    if msg.tournament.id == tournament.id
                        && matches!(msg.inner, MsgContents::ParticipantsUpdate)
                    {
                        msg
                    } else {
                        continue;
                    }
                };

                match msg.inner {
                    MsgContents::ParticipantsUpdate => {
                        let serialized_data =
                            spawn_blocking(get_serializable_data.clone())
                                .await
                                .unwrap();
                        let _ = stream
                            .send(rocket_ws::Message::Text(serialized_data))
                            .await;
                    }
                    _ => unreachable!(),
                }
            }
        })
    }))
}
