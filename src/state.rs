use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::Key;
use diesel::{
    Connection, SqliteConnection,
    connection::TransactionManager,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub key: Key,
}

impl FromRef<AppState> for DbPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.key.clone()
    }
}

#[derive(Clone, Default)]
pub struct TxHandle(
    pub  Arc<
        tokio::sync::Mutex<
            Option<PooledConnection<ConnectionManager<SqliteConnection>>>,
        >,
    >,
);

pub async fn transaction_middleware(
    mut req: axum::extract::Request,
    next: Next,
) -> Response {
    let tx_handle = TxHandle::default();
    req.extensions_mut().insert(tx_handle.clone());

    let response = next.run(req).await;

    let mut guard = tx_handle.0.lock().await;
    if let Some(mut conn) = guard.take() {
        if response.status().is_success()
            || response.status().is_redirection()
            || response.status().is_informational()
        {
            let _ = <PooledConnection<ConnectionManager<SqliteConnection>> as Connection>::TransactionManager::commit_transaction(&mut conn);
        } else {
            let _ = <PooledConnection<ConnectionManager<SqliteConnection>> as Connection>::TransactionManager::rollback_transaction(&mut conn);
        }
    }

    response
}

pub struct Conn<const TX: bool> {
    inner: tokio::sync::OwnedMutexGuard<
        Option<PooledConnection<ConnectionManager<SqliteConnection>>>,
    >,
}

impl<const TX: bool> Deref for Conn<TX> {
    type Target = PooledConnection<ConnectionManager<SqliteConnection>>;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<const TX: bool> DerefMut for Conn<TX> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().unwrap()
    }
}

#[derive(Clone)]
pub struct ThreadSafeConn<const TX: bool> {
    pub inner: Arc<
        tokio::sync::Mutex<
            Option<PooledConnection<ConnectionManager<SqliteConnection>>>,
        >,
    >,
}

#[async_trait]
impl<const TX: bool, S> FromRequestParts<S> for ThreadSafeConn<TX>
where
    S: Send + Sync,
    DbPool: FromRef<S>,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        if TX {
            if let Some(handle) = parts.extensions.get::<TxHandle>() {
                let inner = handle.0.clone();
                {
                    let mut guard = inner.lock().await;
                    if guard.is_none() {
                        let pool = DbPool::from_ref(state);
                        let mut conn = pool.get().map_err(|e| {
                            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                        })?;

                        <PooledConnection<ConnectionManager<SqliteConnection>> as Connection>::TransactionManager::begin_transaction(&mut conn).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                        *guard = Some(conn);
                    }
                }
                Ok(ThreadSafeConn { inner })
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Transaction middleware missing".to_string(),
                ))
            }
        } else {
            #[derive(Clone)]
            struct NonTxHandle(
                Arc<
                    tokio::sync::Mutex<
                        Option<
                            PooledConnection<
                                ConnectionManager<SqliteConnection>,
                            >,
                        >,
                    >,
                >,
            );

            if let Some(handle) = parts.extensions.get::<NonTxHandle>() {
                Ok(ThreadSafeConn {
                    inner: handle.0.clone(),
                })
            } else {
                let pool = DbPool::from_ref(state);
                let conn = pool.get().map_err(|e| {
                    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                })?;
                let inner = Arc::new(tokio::sync::Mutex::new(Some(conn)));
                parts.extensions.insert(NonTxHandle(inner.clone()));
                Ok(ThreadSafeConn { inner })
            }
        }
    }
}

#[async_trait]
impl<const TX: bool, S> FromRequestParts<S> for Conn<TX>
where
    S: Send + Sync,
    DbPool: FromRef<S>,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let ts_conn =
            ThreadSafeConn::<TX>::from_request_parts(parts, state).await?;
        let inner = ts_conn.inner.lock_owned().await;
        Ok(Conn { inner })
    }
}
