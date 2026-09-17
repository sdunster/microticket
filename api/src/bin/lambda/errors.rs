use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum ServerError {
    #[error("could not serialize JSON")]
    Disconnect(#[from] serde_json::Error),
    #[error("error creating response")]
    Response(#[from] http::Error),
}

#[derive(Error, Debug)]
pub(crate) enum ClientError {
    #[error("Could not parse JSON body")]
    Json(#[from] serde_json::Error),
    #[error("POST request must contain a body")]
    EmptyBody,
    #[error("Only GET and POST methods are allowed")]
    MethodNotAllowed,
}
