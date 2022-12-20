pub mod mpc {
    include!("generated.rs");
    // tonic::include_proto!("mpc");
}

use curv::arithmetic::Converter;
use curv::BigInt;
use tokio::sync::mpsc::UnboundedSender;
use tonic::{Response, Status};
use mpc::mpc_server::Mpc;
use mpc::{KeygenRequest, KeygenResponse, SignRequest, SignResponse, CheckResultRequest, CheckResultResponse};
use crate::{KeygenPayload, Request, RequestStatus, RequestType, ResponsePayload, SignPayload};
use crate::server::mpc::check_result_response;

#[derive(Debug)]
pub struct MpcImp {
    tx: UnboundedSender<Request>,
    tx_res: UnboundedSender<ResponsePayload>,
    results_db: sled::Db,
}

impl MpcImp {
    pub(crate) fn new(tx: UnboundedSender<Request>,
           tx_res: UnboundedSender<ResponsePayload>,
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
        let _ = self.tx_res.send(ResponsePayload {
            request_id: request.get_ref().request_id.clone(),
            result: None,
            request_type: RequestType::KEYGEN,
            request_status: RequestStatus::RECEIVED,
        });
        let payload = KeygenPayload {
            request_id: request.get_ref().request_id.clone(),
            public_keys: request.get_ref().participant_public_keys.clone(),
            t: request.get_ref().threshold as u16,
        };
        match self.tx.send(Request::Keygen(payload)) {
            Ok(_) => {
                let resp = KeygenResponse {
                    request_id: request.get_ref().request_id.clone()
                };
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
                let _ = self.tx_res.send(ResponsePayload {
                    request_id: request.get_ref().request_id.clone(),
                    result: None,
                    request_type: RequestType::SIGN,
                    request_status: RequestStatus::RECEIVED,
                });
                let payload = SignPayload {
                    request_id: request.get_ref().request_id.clone(),
                    public_key: request.get_ref().public_key.clone(),
                    participant_public_keys: request.get_ref().participant_public_keys.clone(),
                    message: request.get_ref().hash.clone(),
                };
                match self.tx.send(Request::Sign(payload)) {
                    Ok(_) => {
                        let resp = SignResponse {
                            request_id: request.get_ref().request_id.clone()
                        };
                        Ok(Response::new(resp))
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
        let response = serde_json::from_slice::<ResponsePayload>(&*response);

        match response {
            Ok(r) => {
                let req_type = match r.request_type {
                    RequestType::KEYGEN => {
                        check_result_response::RequestType::Keygen as i32
                    }
                    RequestType::SIGN => {
                        check_result_response::RequestType::Sign as i32
                    }
                };
                let req_stat = match r.request_status {
                    RequestStatus::RECEIVED => {
                        check_result_response::RequestStatus::Received as i32
                    }
                    RequestStatus::PROCESSING => {
                        check_result_response::RequestStatus::Processing as i32
                    }
                    RequestStatus::OFFLINE_STAGE_DONE => {
                        check_result_response::RequestStatus::Processing as i32
                    }
                    RequestStatus::DONE => {
                        check_result_response::RequestStatus::Done as i32
                    }
                    RequestStatus::ERROR => {
                        check_result_response::RequestStatus::Done as i32
                    }
                };
                let resp = CheckResultResponse {
                    request_id: r.request_id,
                    result: r.result.unwrap_or("".to_owned()),
                    request_type: req_type,
                    request_status: req_stat,
                };
                Ok(Response::new(resp))
            }
            Err(_) => not_found
        }
    }
}
