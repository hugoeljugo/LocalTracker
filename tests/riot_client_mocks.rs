use local_tracker::models::MatchHistoryResponse;
use local_tracker::riot_client::{fetch_match_history_mock, fetch_token_mock};

#[test]
fn extrae_access_token_entitlements_token_y_puuid_del_mock() {
    let token = fetch_token_mock().expect("debería parsear mocks/token_response.json");

    assert!(token.access_token.starts_with("eyJ"));
    assert!(token.entitlements_token.starts_with("eyJ"));
    assert_eq!(token.subject, "12345678-abcd-1234-abcd-1234567890ab");
}

#[test]
fn extrae_historial_de_partidas_del_mock() {
    let history = fetch_match_history_mock().expect("debería parsear mocks/match_history.json");

    assert_eq!(history.subject, "12345678-abcd-1234-abcd-1234567890ab");
    assert_eq!(history.history.len(), 1);

    let first_match = &history.history[0];
    assert_eq!(
        first_match.match_id,
        "98765432-fedc-9876-fedc-0987654321fe"
    );
    assert_eq!(first_match.queue_id.as_deref(), Some("competitive"));
}

#[test]
fn tolera_queue_id_nulo_o_ausente_sin_romper_el_parseo() {
    let json = r#"{
        "Subject": "puuid-x",
        "BeginIndex": 0,
        "EndIndex": 1,
        "Total": 1,
        "History": [
            { "MatchID": "match-nulo", "GameStartTime": 1, "QueueID": null },
            { "MatchID": "match-ausente", "GameStartTime": 2 }
        ]
    }"#;

    let parsed: MatchHistoryResponse =
        serde_json::from_str(json).expect("debería tolerar QueueID nulo o ausente");

    assert_eq!(parsed.history[0].queue_id, None);
    assert_eq!(parsed.history[1].queue_id, None);
}
