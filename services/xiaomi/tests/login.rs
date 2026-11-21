use axum::{
    body::Body,
    extract::State,
    http::{header, Response},
    routing::get,
    Json, Router,
};
use homecore_xiaomi::login::Provider;
use serde_json::json;

#[tokio::test]
async fn creates_a_qr_challenge_from_the_provider() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let app = Router::new()
        .route(
            "/longPolling/loginUrl",
            get(|State(base): State<String>| async move {
                Json(json!({"qr":format!("{base}/qr"),"lp":format!("{base}/poll"),"timeout":120}))
            }),
        )
        .route(
            "/qr",
            get(|| async {
                Response::builder()
                    .header(header::CONTENT_TYPE, "image/png")
                    .body(Body::from(vec![137, 80, 78, 71, 13, 10, 26, 10]))
                    .unwrap()
            }),
        )
        .with_state(base.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let result = Provider::new(&base).unwrap().begin().await.unwrap();
    assert_eq!(result.qr_png, "iVBORw0KGgo=");
    server.abort();
}

#[tokio::test]
async fn approved_qr_authenticates_without_returning_the_cloud_token_to_the_ui() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let app=Router::new().route("/longPolling/loginUrl",get(|State(base):State<String>|async move {Json(json!({"qr":format!("{base}/qr"),"lp":format!("{base}/poll"),"timeout":120}))}))
        .route("/qr",get(||async {Response::builder().header(header::CONTENT_TYPE,"image/png").body(Body::from(vec![137,80,78,71,13,10,26,10])).unwrap()}))
        .route("/poll",get(|State(base):State<String>|async move {Json(json!({"userId":"123","ssecurity":"c2VjdXJpdHk=","location":format!("{base}/token")}))}))
        .route("/token",get(||async {Response::builder().header(header::SET_COOKIE,"serviceToken=fixture-session; Path=/; HttpOnly").body(Body::empty()).unwrap()})).with_state(base.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = Provider::new(&base).unwrap();
    let challenge = provider.begin().await.unwrap();
    assert!(provider.poll(&challenge).await.unwrap().is_authenticated());
    server.abort();
}

#[tokio::test]
async fn authenticated_setup_discovers_gateway_without_exposing_its_token() {
    use axum::extract::RawQuery;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ring::digest::{digest, SHA256};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let app=Router::new().route("/longPolling/loginUrl",get(|State(base):State<String>|async move {Json(json!({"qr":format!("{base}/qr"),"lp":format!("{base}/poll"),"timeout":120}))}))
      .route("/qr",get(||async {Response::builder().header(header::CONTENT_TYPE,"image/png").body(Body::from(vec![137,80,78,71,13,10,26,10])).unwrap()}))
      .route("/poll",get(|State(base):State<String>|async move {Json(json!({"userId":"123","ssecurity":"Zml4dHVyZS1zZWN1cml0eSE=","location":format!("{base}/token")}))}))
      .route("/token",get(||async {Response::builder().header(header::SET_COOKIE,"serviceToken=fixture-session; Path=/; HttpOnly").body(Body::empty()).unwrap()}))
      .route("/app/home/device_list",axum::routing::post(|RawQuery(query):RawQuery|async move {
          let parsed=reqwest::Url::parse(&format!("http://localhost/?{}",query.unwrap())).unwrap();let nonce=parsed.query_pairs().find(|(k,_)|k=="_nonce").unwrap().1.into_owned();
          let mut material=STANDARD.decode("Zml4dHVyZS1zZWN1cml0eSE=").unwrap();material.extend(STANDARD.decode(nonce).unwrap());let hash=digest(&SHA256,&material);let key:[u8;32]=hash.as_ref().try_into().unwrap();
          let body=json!({"result":{"list":[{"did":"123456789","token":"fedcba9876543210fedcba9876543210","isOnline":true}]}});
          STANDARD.encode(homecore_xiaomi::cloud::legacy_encrypt(&key,body.to_string().as_bytes()))
      }))
      .route("/app/v2/{*path}",axum::routing::post(|axum::extract::Path(path):axum::extract::Path<String>,RawQuery(query):RawQuery|async move {
          let parsed=reqwest::Url::parse(&format!("http://localhost/?{}",query.unwrap())).unwrap();let nonce=parsed.query_pairs().find(|(k,_)|k=="_nonce").unwrap().1.into_owned();
          let mut material=STANDARD.decode("Zml4dHVyZS1zZWN1cml0eSE=").unwrap();material.extend(STANDARD.decode(nonce).unwrap());let hash=digest(&SHA256,&material);let key:[u8;32]=hash.as_ref().try_into().unwrap();
          let body=if path=="homeroom/gethome" {json!({"result":{"homelist":[{"id":42}]}})} else {json!({"result":{"device_info":[
              {"did":"123456789","name":"Gateway","model":"lumi.gateway.mcn001","localip":"192.168.10.2","token":"0123456789abcdef0123456789abcdef"},
              {"did":"223456789","name":"Vacuum","model":"ijai.vacuum.v10","localip":"192.168.10.3","token":"fedcba9876543210fedcba9876543210"},
              {"did":"323456789","name":"Unknown plug","model":"example.plug.v1","localip":"192.168.10.4","token":"fedcba9876543210fedcba9876543210"},
              {"did":"423456789","name":"Vacuum without token","model":"ijai.vacuum.v10","localip":"192.168.10.5"}
          ]}})};
          STANDARD.encode(homecore_xiaomi::cloud::legacy_encrypt(&key,body.to_string().as_bytes()))
      })).with_state(base.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = Provider::new(&base).unwrap();
    let challenge = provider.begin().await.unwrap();
    let auth = provider.poll(&challenge).await.unwrap();
    let gateways = provider.discover(&auth, "cn").await.unwrap();
    assert_eq!(gateways.len(), 1);
    assert_eq!(gateways[0].name, "Gateway");
    assert_eq!(gateways[0].model, "lumi.gateway.mcn001");
    assert_eq!(gateways[0].address, "192.168.10.2");
    let root = std::env::temp_dir().join(format!("xiaomi-discovered-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let store = homecore_xiaomi::store::Store::new(&root);
    gateways[0].save_configuration(&store).unwrap();
    let saved = store.load_gateway().unwrap().unwrap();
    assert_eq!(saved.address(), "192.168.10.2");
    assert_eq!(saved.model(), "lumi.gateway.mcn001");
    assert_eq!(saved.device_id(), Some("123456789"));
    assert_eq!(
        saved.alternate_token(),
        Some("fedcba9876543210fedcba9876543210")
    );
    assert_eq!(saved.token(), "0123456789abcdef0123456789abcdef");
    assert_eq!(saved.cloud_comparison(), Some("different"));
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::create_dir(&root).unwrap();
    let setup = homecore_xiaomi::setup::Setup::new(
        Provider::new(&base).unwrap(),
        homecore_xiaomi::store::Store::new(&root),
        "cn",
    );
    assert_eq!(setup.status().await, "unconfigured");
    let reply = homecore_xiaomi::actions::dispatch(&setup, "sign_in_status")
        .await
        .unwrap();
    assert_eq!(reply["status"], "ok");
    assert_eq!(reply["setup_status"], "unconfigured");
    let invalid = homecore_xiaomi::actions::dispatch_with_params(
        &setup,
        "begin_sign_in",
        &json!({"region":"invalid"}),
    )
    .await
    .unwrap();
    assert_eq!(invalid["status"], "error");
    assert_eq!(setup.status().await, "unconfigured");
    let reply = homecore_xiaomi::actions::dispatch(&setup, "begin_sign_in")
        .await
        .unwrap();
    assert!(reply["qr_png"].as_str().unwrap().starts_with("iVBORw0KGgo"));
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while setup.status().await == "waiting" {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(setup.status().await, "gateway_saved");
    assert!(homecore_xiaomi::store::Store::new(&root)
        .load_gateway()
        .unwrap()
        .is_some());
    let restarted = homecore_xiaomi::setup::Setup::new(
        Provider::new(&base).unwrap(),
        homecore_xiaomi::store::Store::new(&root),
        "cn",
    );
    assert_eq!(restarted.status().await, "gateway_saved");
    std::fs::write(
        root.join("adapters.json"),
        br#"[
            {"plugin_id":"plugin.xiaomi-gateway","models":["lumi.gateway.mcn001"]},
            {"plugin_id":"plugin.xiaomi-vacuum","models":["ijai.vacuum.v10"]}
        ]"#,
    )
    .unwrap();
    restarted.begin().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while restarted.status().await == "waiting" {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let reply = homecore_xiaomi::actions::dispatch(&restarted, "sign_in_status")
        .await
        .unwrap();
    assert_eq!(reply["setup_status"], "devices_found");
    assert_eq!(
        reply["devices"],
        json!([
            {"id":"123456789","name":"Gateway","model":"lumi.gateway.mcn001","adapters":["plugin.xiaomi-gateway"]},
            {"id":"223456789","name":"Vacuum","model":"ijai.vacuum.v10","adapters":["plugin.xiaomi-vacuum"]}
        ])
    );
    let public = reply.to_string();
    assert!(!public.contains("fixture-session"));
    assert!(!public.contains("0123456789abcdef0123456789abcdef"));
    assert!(!public.contains("fedcba9876543210fedcba9876543210"));
    std::fs::remove_dir_all(root).unwrap();
    server.abort();
}

#[tokio::test]
async fn pending_qr_can_be_retrieved_from_another_setup_page() {
    use homecore_xiaomi::{actions, setup::Setup, store::Store};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route(
            "/longPolling/loginUrl",
            get(|State(base): State<String>| async move {
                Json(json!({"qr":format!("{base}/qr"),"lp":format!("{base}/poll"),"timeout":120}))
            }),
        )
        .route(
            "/qr",
            get(|| async {
                Response::builder()
                    .header(header::CONTENT_TYPE, "image/png")
                    .body(Body::from(vec![137, 80, 78, 71, 13, 10, 26, 10]))
                    .unwrap()
            }),
        )
        .route(
            "/poll",
            get(|| async {
                std::future::pending::<()>().await;
                ""
            }),
        )
        .with_state(base.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let root = std::env::temp_dir().join(format!("xiaomi-pending-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let setup = Setup::new(Provider::new(&base).unwrap(), Store::new(&root), "cn");
    let begun = actions::dispatch(&setup, "begin_sign_in").await.unwrap();
    let resumed = actions::dispatch(&setup, "sign_in_status").await.unwrap();
    assert_eq!(resumed["setup_status"], "waiting");
    assert_eq!(resumed["qr_png"], begun["qr_png"]);
    server.abort();
    std::fs::remove_dir(root).unwrap();
}

#[tokio::test]
async fn failed_sign_in_again_preserves_the_existing_gateway() {
    use homecore_xiaomi::{
        setup::Setup,
        store::{GatewayConfig, Store},
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route(
            "/longPolling/loginUrl",
            get(|State(base): State<String>| async move {
                Json(json!({"qr":format!("{base}/qr"),"lp":format!("{base}/poll"),"timeout":120}))
            }),
        )
        .route(
            "/qr",
            get(|| async {
                Response::builder()
                    .header(header::CONTENT_TYPE, "image/png")
                    .body(Body::from(vec![137, 80, 78, 71, 13, 10, 26, 10]))
                    .unwrap()
            }),
        )
        .route(
            "/poll",
            get(|| async { Json(json!({"error":"cancelled"})) }),
        )
        .with_state(base.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let root = std::env::temp_dir().join(format!("xiaomi-reauth-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let store = Store::new(&root);
    store
        .save_gateway(
            &GatewayConfig::new(
                "192.168.1.12",
                "lumi.gateway.mgl03",
                "0123456789abcdef0123456789abcdef",
            )
            .unwrap(),
        )
        .unwrap();
    let setup = Setup::new(Provider::new(&base).unwrap(), store, "cn");
    assert_eq!(setup.status().await, "gateway_saved");
    setup.begin().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while setup.status().await == "waiting" {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(setup.status().await, "error");
    assert_eq!(
        Store::new(&root).load_gateway().unwrap().unwrap().address(),
        "192.168.1.12"
    );
    server.abort();
    std::fs::remove_file(root.join("gateway.json")).unwrap();
    std::fs::remove_dir(root).unwrap();
}
