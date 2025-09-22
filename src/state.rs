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
    Request, Response,
    fairing::{Fairing, Info, Kind},
    http::StatusClass,
    outcome::Outcome,
    request::{self, FromRequest},
};

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

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
        let conn: &Option<ThreadSafeConn<true>> = req.local_cache(|| None);

        if let Some(conn) = conn {
            let mut conn = conn.inner.try_lock().unwrap();

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

pub struct Conn<const TX: bool> {
    inner: tokio::sync::OwnedMutexGuard<
        PooledConnection<ConnectionManager<SqliteConnection>>,
    >,
}

impl<const TX: bool> Deref for Conn<TX> {
    type Target = PooledConnection<ConnectionManager<SqliteConnection>>;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<const TX: bool> DerefMut for Conn<TX> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

#[rocket::async_trait]
impl<'r, const TX: bool> FromRequest<'r> for Conn<TX> {
    type Error = ();
    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, Self::Error> {
        request::Outcome::Success({
            let conn = request.guard::<ThreadSafeConn<TX>>().await;
            match conn {
                rocket::outcome::Outcome::Success(conn) => Conn {
                    inner: conn.inner.clone().try_lock_owned().unwrap(),
                },
                rocket::outcome::Outcome::Error(e) => return Outcome::Error(e),
                rocket::outcome::Outcome::Forward(f) => {
                    return Outcome::Forward(f);
                }
            }
        })
    }
}

#[derive(Clone)]
pub struct ThreadSafeConn<const TX: bool> {
    pub inner: Arc<
        tokio::sync::Mutex<
            PooledConnection<ConnectionManager<SqliteConnection>>,
        >,
    >,
}

#[rocket::async_trait]
impl<'r, const TX: bool> FromRequest<'r> for ThreadSafeConn<TX> {
    type Error = ();
    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, Self::Error> {
        request::Outcome::Success({
            request
                .local_cache_async::<Option<ThreadSafeConn<TX>>, _>(async {
                    let pool = request.rocket().state::<DbPool>().unwrap().clone();

                    let mut conn = tokio::task::spawn_blocking(move || pool.get().unwrap())
                        .await
                        .unwrap();

                    if TX {
                        <PooledConnection<ConnectionManager<SqliteConnection>> as diesel::Connection>
                            ::TransactionManager
                            ::begin_transaction(&mut conn).unwrap();
                    }

                    let t = Arc::new(tokio::sync::Mutex::new(conn));

                    Some(ThreadSafeConn { inner: t })
                })
                .await
                .clone()
                .unwrap()
        })
    }
}
