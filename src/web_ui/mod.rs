pub mod admin;
pub mod api_keys;
pub mod auth;
pub mod kiro_setup;
pub mod session;

use axum::{
    middleware as axum_middleware,
    routing::{delete, get, patch, post},
    Router,
};

use crate::routes::AppState;

pub fn web_ui_routes(state: AppState) -> Router {
    let session_protected = Router::new()
        // Auth
        .route("/api/auth/me", get(auth::me_handler))
        .route("/api/auth/logout", post(auth::logout_handler))
        // API Keys
        .route("/api/keys", get(api_keys::list_keys_handler))
        .route("/api/keys", post(api_keys::create_key_handler))
        .route("/api/keys/:id", delete(api_keys::delete_key_handler))
        // Kiro setup
        .route("/api/kiro/setup", post(kiro_setup::setup_handler))
        .route("/api/kiro/poll", post(kiro_setup::poll_handler))
        .route("/api/kiro/status", get(kiro_setup::status_handler))
        .route("/api/kiro/token", delete(kiro_setup::delete_token_handler))
        // Admin
        .route("/api/admin/users", get(admin::list_users_handler))
        .route("/api/admin/users/:id", delete(admin::delete_user_handler))
        .route("/api/admin/users/:id/approve", post(admin::approve_user_handler))
        .route("/api/admin/users/:id/reject", post(admin::reject_user_handler))
        .route("/api/admin/pool", get(admin::list_pool_handler))
        .route("/api/admin/pool", post(admin::add_pool_handler))
        .route("/api/admin/pool/setup", post(admin::pool_setup_handler))
        .route("/api/admin/pool/poll", post(admin::pool_poll_handler))
        .route("/api/admin/pool/:id", delete(admin::delete_pool_handler))
        .route("/api/admin/pool/:id", patch(admin::toggle_pool_handler))
        .route("/api/admin/usage", get(admin::usage_handler))
        .route("/api/admin/accounts", get(admin::list_accounts_handler))
        .route("/api/admin/accounts/:id", patch(admin::toggle_account_handler))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            session::session_middleware,
        ))
        .with_state(state.clone());

    // Public auth routes (no session required)
    let public = Router::new()
        .route("/api/auth/register", post(auth::register_handler))
        .route("/api/auth/login", post(auth::login_handler))
        .with_state(state);

    Router::new()
        .nest("/_ui", session_protected)
        .nest("/_ui", public)
}
