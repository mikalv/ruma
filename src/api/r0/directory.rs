//! Endpoints for managing room aliases.

use bodyparser;
use iron::{Chain, Handler, IronError, IronResult, Plugin, Request, Response};
use iron::status::Status;
use router::Router;

use config::Config;
use db::DB;
use error::APIError;
use middleware::{AccessTokenAuth, JsonRequest};
use modifier::SerializableResponse;
use room_alias::{RoomAlias, NewRoomAlias};

#[derive(Debug, Serialize)]
struct GetDirectoryRoomResponse {
    room_id: String,
    servers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PutDirectoryRoomRequest {
    pub room_id: String,
}

/// The /directory/room/{roomAlias} endpoint when using the GET method.
pub struct GetDirectoryRoom;

/// The /directory/room/{roomAlias} endpoint for the DELETE method.
pub struct DeleteDirectoryRoom;

/// The /directory/room/{roomAlias} endpoint when using the PUT method.
pub struct PutDirectoryRoom;

impl GetDirectoryRoom {
    /// Create a `GetDirectoryRoom`.
    pub fn chain() -> Chain {
        Chain::new(GetDirectoryRoom)
    }
}

impl DeleteDirectoryRoom {
    /// Create a `DeleteDirectoryRoom` with necessary middleware.
    pub fn chain() -> Chain {
        let mut chain = Chain::new(DeleteDirectoryRoom);

        chain.link_before(AccessTokenAuth);

        chain
    }
}

impl PutDirectoryRoom {
    /// Create a `PutDirectoryRoom` with necessary middleware.
    pub fn chain() -> Chain {
        let mut chain = Chain::new(PutDirectoryRoom);

        chain.link_before(JsonRequest);
        chain.link_before(AccessTokenAuth);

        chain
    }
}

impl Handler for GetDirectoryRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let params = request.extensions.get::<Router>().expect("Params object is missing").clone();

        let room_alias_name = params.find("room_alias").ok_or(APIError::not_found())?;

        let connection = DB::from_request(request)?;

        let room_alias = RoomAlias::find_by_alias(&connection, room_alias_name)?;

        let response = GetDirectoryRoomResponse {
            room_id: room_alias.room_id,
            servers: room_alias.servers,
        };

        Ok(Response::with((Status::Ok, SerializableResponse(response))))
    }
}

impl Handler for DeleteDirectoryRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let params = request.extensions.get::<Router>().expect("Params object is missing").clone();

        let room_alias_name = params.find("room_alias").ok_or(APIError::not_found())?;

        let connection = DB::from_request(request)?;

        RoomAlias::delete(&connection, room_alias_name)?;

        // Respond with an empty JSON object
        Ok(Response::with((Status::Ok, "{}")))
    }
}

impl Handler for PutDirectoryRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let params = request.extensions.get::<Router>().expect("Params object is missing").clone();

        let room_alias_name = params.find("room_alias").ok_or(APIError::not_found())?;

        let parsed_request = request.get::<bodyparser::Struct<PutDirectoryRoomRequest>>();
        let room_id = if let Ok(Some(api_request)) = parsed_request {
            api_request.room_id
        } else {
            let error = APIError::bad_json();

            return Err(IronError::new(error.clone(), error));
        };

        let connection = DB::from_request(request)?;
        let config = Config::from_request(request)?;

        let new_room_alias = NewRoomAlias {
            alias: room_alias_name.to_string(),
            room_id: room_id,
            servers: vec![config.domain.to_string()],
        };

        // TODO: Fix returned status if the alias could not be created because an alias with that
        // name already exists. Should be `Status::Conflict` (409) according to the spec.
        // See #77 for discussion.
        RoomAlias::create(&connection, &new_room_alias)?;

        Ok(Response::with(Status::Ok))
    }
}

#[cfg(test)]
mod tests {
    use test::Test;
    use iron::status::Status;

    #[test]
    fn get_room_alias() {
        let test = Test::new();
        let access_token = test.create_access_token();

        let create_room_path = format!("/_matrix/client/r0/createRoom?access_token={}",
                                       access_token);
        let response = test.post(&create_room_path, r#"{"room_alias_name": "my_room"}"#);
        let room_id = response.json().find("room_id").unwrap().as_string();

        let response = test.get("/_matrix/client/r0/directory/room/my_room");

        assert_eq!(response.json().find("room_id").unwrap().as_string(), room_id);
        assert!(response.json().find("servers").unwrap().is_array());
    }

    #[test]
    fn get_unknown_room_alias() {
        let test = Test::new();
        let access_token = test.create_access_token();

        let create_room_path = format!("/_matrix/client/r0/createRoom?access_token={}",
                                       access_token);
        let _ = test.post(&create_room_path, r#"{"room_alias_name": "my_room"}"#);

        let response = test.get("/_matrix/client/r0/directory/room/no_room");

        assert_eq!(response.status, Status::NotFound);
        assert_eq!(
            response.json().find("errcode").unwrap().as_string().unwrap(),
            "M_NOT_FOUND"
        );
    }

    #[test]
    fn delete_room_alias() {
        let test = Test::new();
        let access_token = test.create_access_token();

        // Create a room
        let create_room_path = format!("/_matrix/client/r0/createRoom?access_token={}",
                                       access_token);
        test.post(&create_room_path, r#"{"room_alias_name": "my_room"}"#);

        // Delete the room alias
        let delete_room_path = format!("/_matrix/client/r0/directory/room/my_room?access_token={}",
                                       access_token);
        let response = test.delete(&delete_room_path);

        assert_eq!(response.status, Status::Ok);

        // Make sure the room no longer exists
        let response = test.get("/_matrix/client/r0/directory/room/my_room");

        assert_eq!(response.status, Status::NotFound);
        assert_eq!(
            response.json().find("errcode").unwrap().as_string().unwrap(),
            "M_NOT_FOUND"
        );
    }

    #[test]
    fn put_room_alias() {
        let test = Test::new();
        let access_token = test.create_access_token();

        let create_room_path = format!("/_matrix/client/r0/createRoom?access_token={}",
                                       access_token);
        let response = test.post(&create_room_path, "{}");
        let room_id = response.json().find("room_id").unwrap().as_string().unwrap();

        let put_room_alias_path = format!(
            "/_matrix/client/r0/directory/room/my_room?access_token={}", access_token
        );
        let put_room_alias_body = format!(r#"{{"room_id": "{}"}}"#, room_id);
        let response = test.put(&put_room_alias_path, &put_room_alias_body);

        assert_eq!(response.status, Status::Ok);

        let response = test.get("/_matrix/client/r0/directory/room/my_room");

        assert_eq!(response.json().find("room_id").unwrap().as_string().unwrap(), room_id);
        assert!(response.json().find("servers").unwrap().is_array());
    }

    #[test]
    fn put_existing_room_alias() {
        let test = Test::new();
        let access_token = test.create_access_token();

        let create_room_path = format!("/_matrix/client/r0/createRoom?access_token={}",
                                       access_token);
        let response = test.post(&create_room_path, r#"{"room_alias_name": "my_room"}"#);
        let room_id = response.json().find("room_id").unwrap().as_string().unwrap();

        let put_room_alias_path = format!(
            "/_matrix/client/r0/directory/room/my_room?access_token={}", access_token
        );
        let put_room_alias_body = format!(r#"{{"room_id": "{}"}}"#, room_id);
        let response = test.put(&put_room_alias_path, &put_room_alias_body);

        // TODO: Fix returned status. Should be `Status::Conflict` (409) according to the spec.
        assert_eq!(response.status, Status::InternalServerError);
        assert_eq!(
            response.json().find("errcode").unwrap().as_string().unwrap(),
            "M_UNKNOWN"
        );
    }
}