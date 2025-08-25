use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use diesel::{
    SqliteConnection,
    connection::TransactionManager,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use rocket::{
    Request, Response, State,
    fairing::{Fairing, Info, Kind},
    http::StatusClass,
    outcome::Outcome,
    request::{self, FromRequest},
};
use tokio::sync::MutexGuard;

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

#[derive(Clone)]
/// This struct implements [`FromRequest`], and can be used to obtain a
/// connection from the pool. This connection will be re-used across the
/// request (i.e. we use a single connection for the course of the whole
/// request). All queries per request (when submitted to this object) are
/// carried out as part of a transaction (to later be committed using
/// [`TxCommitFairing`]).
pub struct Conn {
    inner: Arc<
        tokio::sync::Mutex<
            PooledConnection<ConnectionManager<SqliteConnection>>,
        >,
    >,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Conn {
    type Error = ();
    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, Self::Error> {
        request::Outcome::Success(
            request
                .local_cache_async(async {
                    let pool = request.rocket().state::<&State<DbPool>>().unwrap();

                    let mut conn = tokio::task::spawn_blocking(|| pool.get().unwrap())
                        .await
                        .unwrap();

                    <PooledConnection<ConnectionManager<SqliteConnection>> as diesel::Connection>
                        ::TransactionManager
                        ::begin_transaction(&mut conn).unwrap();

                    Some(Conn {
                        inner: Arc::new(tokio::sync::Mutex::new(conn)),
                    })
                })
                .await
                .clone()
                .unwrap(),
        )
    }
}

impl Conn {
    pub async fn get(&self) -> LockedConn<'_> {
        LockedConn {
            lock: self.inner.lock().await,
        }
    }

    pub fn get_sync(&self) -> LockedConn<'_> {
        LockedConn {
            lock: self.inner.blocking_lock(),
        }
    }
}

/// Similar to [`Conn`], except that it acquires the lock.
pub struct LockedConn<'r> {
    lock: MutexGuard<'r, PooledConnection<ConnectionManager<SqliteConnection>>>,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for LockedConn<'r> {
    type Error = ();
    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, Self::Error> {
        request::Outcome::Success({
            let conn = request.guard::<&State<Conn>>().await;
            match conn {
                rocket::outcome::Outcome::Success(conn) => conn.get().await,
                rocket::outcome::Outcome::Error(e) => return Outcome::Error(e),
                rocket::outcome::Outcome::Forward(f) => {
                    return Outcome::Forward(f);
                }
            }
        })
    }
}

impl Deref for LockedConn<'_> {
    type Target = PooledConnection<ConnectionManager<SqliteConnection>>;

    fn deref(&self) -> &Self::Target {
        self.lock.deref()
    }
}

impl DerefMut for LockedConn<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.lock.deref_mut()
    }
}

/// This fairing commits opened transactions, after each request has been
/// handled.
pub struct TxCommitFairing;

#[rocket::async_trait]
impl Fairing for TxCommitFairing {
    fn info(&self) -> Info {
        Info {
            name: "tx_commit",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(
        &self,
        req: &'r Request<'_>,
        res: &mut Response<'r>,
    ) {
        let conn: &Option<Conn> = req.local_cache(|| None);

        if let Some(conn) = conn {
            let mut conn = conn.inner.lock().await;

            if matches!(
                res.status().class(),
                StatusClass::Success
                    | StatusClass::Redirection
                    | StatusClass::Informational
            ) {
                <PooledConnection<ConnectionManager<SqliteConnection>> as diesel::Connection>
                    ::TransactionManager
                    ::commit_transaction(&mut conn).unwrap();
            } else {
                <PooledConnection<ConnectionManager<SqliteConnection>> as diesel::Connection>
                    ::TransactionManager
                    ::rollback_transaction(&mut conn).unwrap();
            }
        }
    }
}
