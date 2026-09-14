use futures_util::StreamExt;

use crate::serve::grpc::compressor_api::{
    self, PageBlocksWithCtxRequest, StreamUpdatesWithCtxRequest,
};

#[tokio::test]
#[ignore]
async fn test_grpc_page_block_with_context() {
    let mut client =
        compressor_api::sync_service_client::SyncServiceClient::connect("http://localhost:50051")
            .await
            .unwrap();

    let request = PageBlocksWithCtxRequest {
        cursor: Some(compressor_api::BlockRef {
            slot: 59780,
            hash: hex::decode("c3e7b3fa9b92af8a7bc2778d79af5fb797c0a10c406058c64f943c8693a56eba")
                .unwrap(),
        }),
        max_items: 1,
    };

    let res = client.page_blocks_with_context(request).await;

    // let res = res
    //     .unwrap()
    //     .into_inner()
    //     .blocks
    //     .into_iter()
    //     .map(|x| x.r#ref)
    //     .collect::<Vec<_>>();

    println!("{:?}", res);
}

#[tokio::test]
#[ignore]
async fn test_grpc_stream_block_with_context() {
    let mut client =
        compressor_api::sync_service_client::SyncServiceClient::connect("http://localhost:50051")
            .await
            .unwrap();

    let request = StreamUpdatesWithCtxRequest {
        intersects: vec![compressor_api::BlockRef {
            slot: 820,
            hash: hex::decode("8bb2b6566f00d13fe954100b2d57f38c76a13d23dc7cb4073a206b8de1107de4")
                .unwrap(),
        }],
    };

    let mut res = client
        .stream_updates_with_context(request)
        .await
        .unwrap()
        .into_inner();

    while let Some(result) = res.next().await {
        println!("got stream update: {result:?}");
    }

    println!("{:?}", res);
}
