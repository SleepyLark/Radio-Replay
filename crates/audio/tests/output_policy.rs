// Integration tests link the ordinary library, without its cfg(test). Verify
// that the environment hook cannot silence a normal build accidentally.
#[test]
#[cfg(not(all(debug_assertions, feature = "smoke-test-output")))]
fn ordinary_library_ignores_test_output_environment() {
    if std::env::var_os("RADIO_REPLAY_OUTPUT_POLICY_CHILD").is_some() {
        assert_eq!(
            std::env::var("RADIO_REPLAY_AUDIO_TEST_NULL_OUTPUT").unwrap(),
            "1"
        );
        assert!(!radio_replay_audio::test_output_enabled());
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ordinary_library_ignores_test_output_environment",
        ])
        .env("RADIO_REPLAY_AUDIO_TEST_NULL_OUTPUT", "1")
        .env("RADIO_REPLAY_OUTPUT_POLICY_CHILD", "1")
        .status()
        .unwrap();
    assert!(status.success());
}
