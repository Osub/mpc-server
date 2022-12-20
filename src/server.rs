// pub mod mpc {
//     include!("generated.rs");
//     // tonic::include_proto!("mpc");
// }

use curv::arithmetic::Converter;
use curv::BigInt;
use tokio::sync::mpsc::UnboundedSender;
use tonic::{Response, Status};
use crate::pb::mpc::mpc_server::Mpc;
use crate::pb::mpc::{KeygenRequest, KeygenResponse, SignRequest, SignResponse, CheckResultRequest, CheckResultResponse};
// use crate::{KeygenPayload, Request, RequestStatus, RequestType, ResponsePayload, SignPayload};
use crate::pb::mpc::check_result_response;
use crate::pb::types::request::Request;

#[derive(Debug)]
pub struct MpcImp {
    tx: UnboundedSender<Request>,
    tx_res: UnboundedSender<CheckResultResponse>,
    results_db: sled::Db,
}

impl MpcImp {
    pub(crate) fn new(tx: UnboundedSender<Request>,
           tx_res: UnboundedSender<CheckResultResponse>,
           results_db: sled::Db) -> Self {
        return Self {
            tx,
            tx_res,
            results_db,
        };
    }
}


#[tonic::async_trait]
impl Mpc for MpcImp {
    async fn keygen(
        &self,
        request: tonic::Request<KeygenRequest>,
    ) -> Result<Response<KeygenResponse>, Status> {
        let already_exists = self.results_db.contains_key(request.get_ref().request_id.as_bytes()).map_or(false, |x| x);
        if already_exists {
            return Err(Status::already_exists(format!("A request of id \"{}\" already exists.", request.get_ref().request_id)));
        }
        let request_id = request.get_ref().request_id.clone();
        let _ = self.tx_res.send(CheckResultResponse {
            request_id: request_id.clone(),
            result: "".to_owned(),
            request_type: check_result_response::RequestType::Keygen as i32,
            request_status: check_result_response::RequestStatus::Received as i32,
        });
        match self.tx.send(Request::Keygen(request.into_inner())) {
            Ok(_) => {
                let resp = KeygenResponse { request_id };
                Ok(Response::new(resp))
            }
            Err(_) => {
                Err(Status::internal("Failed to queue the request."))
            }
        }
    }

    async fn sign(&self, request: tonic::Request<SignRequest>) -> Result<Response<SignResponse>, Status> {
        let already_exists = self.results_db.contains_key(request.get_ref().request_id.as_bytes()).map_or(false, |x| x);
        if already_exists {
            return Err(Status::already_exists(format!("A request of id \"{}\" already exists.", request.get_ref().request_id)));
        }
        match BigInt::from_hex(&*request.get_ref().hash) {
            Ok(_) => {

                let request_id = request.get_ref().request_id.clone();
                let _ = self.tx_res.send(CheckResultResponse {
                    request_id: request_id.clone(),
                    result: "".to_owned(),
                    request_type: check_result_response::RequestType::Sign as i32,
                    request_status: check_result_response::RequestStatus::Received as i32,
                });
                match self.tx.send(Request::Sign(request.into_inner())) {
                    Ok(_) => {
                        Ok(Response::new(SignResponse {request_id}))
                    }
                    Err(_) => {
                        Err(Status::internal("Failed to queue the request."))
                    }
                }
            }
            Err(_) => {
                Err(Status::invalid_argument("Supplied hash is not a valid hash."))
            }
        }
    }

    async fn check_result(&self, request: tonic::Request<CheckResultRequest>) -> Result<Response<CheckResultResponse>, Status> {
        let response = self.results_db.get(request.get_ref().request_id.as_bytes());
        // let not_found = "{\"error\": \"Not found\"}";
        // let not_found = HttpResponse::NotFound().content_type("application/json").body(not_found);
        let not_found = Err(Status::not_found("Not found"));
        if response.is_err() {
            return not_found;
        }
        if response.as_ref().unwrap().is_none() {
            return not_found;
        }
        let response = response.unwrap().unwrap();
        // let response = String::from_utf8(response.to_vec());
        let response = serde_json::from_slice::<CheckResultResponse>(&*response);

        match response {
            Ok(r) => {
                Ok(Response::new(r))
            }
            Err(_) => not_found
        }
    }
}
