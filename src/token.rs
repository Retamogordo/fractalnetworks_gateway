

use rocket::http::{Status};
use rocket::request::{self, FromRequest, Outcome, Request};


#[derive(Clone, Debug)]
pub struct Token {
    value: String,
}

impl Token {
    pub fn new(value: &str) -> Self {
        Token {
            value: value.to_string(),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Token {
    type Error = &'static str;
    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let token = req.rocket().state::<Token>().unwrap();
        let valid_token = req
            .headers()
            .get("Token")
            .any(|header| header == &token.value);
        if valid_token {
            Outcome::Success(token.clone())
        } else {
            Outcome::Failure((Status::Unauthorized, "Invalid token supplied"))
        }
    }
}
