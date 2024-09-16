use tonic::codegen::tokio_stream::StreamExt;

use crate::helloworld::greeter_client::GreeterClient;
use crate::helloworld::HelloRequest;

pub mod helloworld {
    tonic::include_proto!("helloworld");
}

#[tokio::main]
async fn main() {
    let mut client = GreeterClient::connect("http://[::1]:50051").await.expect("should create client");

    let res = client.say_hello(HelloRequest {
        name: "client".into()
    }).await;

    dbg!(res);
}
