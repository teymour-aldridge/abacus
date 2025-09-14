use crate::{
    state::Conn,
    tournaments::WEBSOCKET_SCHEME,
    util_resp::{StandardResponse, success},
};
use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{State, futures::SinkExt, get};
use tokio::{sync::broadcast::Receiver, task::spawn_blocking};

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::tournaments,
    state::DbPool,
    template::Page,
    tournaments::{Tournament, participants::TournamentParticipants},
};

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
        var table = new Tabulator('#participants', {{
            layout:"fitColumns",
            columnDefaults:{{
              resizable:true,
            }},
            data:[],
            columns:[
                {{title:"id", field:"ID"}},
                {{title:"name", field:"Name"}},
                {{title:"inst", field:"Institution"}},
            ],
            rowFormatter:function(row){{
               var holderEl = document.createElement("div");
               var tableEl = document.createElement("div");

               holderEl.style.boxSizing = "border-box";
               holderEl.style.padding = "10px 30px 10px 10px";
               holderEl.style.borderTop = "1px solid #333";
               holderEl.style.borderBotom = "1px solid #333";

               tableEl.style.border = "1px solid #333";

               holderEl.appendChild(tableEl);

               row.getElement().appendChild(holderEl);

               var subTable = new Tabulator(tableEl, {{
                   layout:"fitColumns",
                   data:row.getData().serviceHistory,
                   columns:[
                    {{title:"ID", field:"id"}},
                    {{title:"Name", field:"name", sorter: "string"}},
                    {{title:"Email", field:"email"}},
                    {{title:"Private URL", field:"private_url"}}
                   ]
               }})
            }},
        }});

        ws = new WebSocket(`{WEBSOCKET_SCHEME}${{window.location.host}}/tournaments/${{{tid}}}/participants?channel`);

        ws.onmessage = function(event) {{
            let data = JSON.parse(event.data);
            table.replaceData(data);
        }};

        ws.onerror = function(event) {{
            console.error("WebSocket error observed:", event);
            alert("WebSocket connection failed. Please refresh the page or try again later.");
        }};
        </script>
        "#,
    );

    success(
        Page::new()
            .user(user)
            .tournament(tournament)
            .body(maud! {
                div #participants {}
                (script)
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
                .check_user_is_tab_dir(&user.id, &mut *conn)
                .is_err()
            {
                return None;
            } else {
                return Some(tournament);
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
                    let msg = rx.recv().await.unwrap();
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
