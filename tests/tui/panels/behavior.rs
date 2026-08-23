use super::super::*;

#[test]
fn behavior_panel_renders_all_behavior_sections_and_sample_rows() {
    let text = render_behavior_text(sample_behavior_payload(), 160, 40);

    for expected in [
        "Behavior",
        "Activity",
        "Tools",
        "Optimize",
        "Model Comparison",
        "coding",
        "Read",
        "(non-tool)",
        "gpt-5",
        "sonnet",
        "Repeated reads",
    ] {
        assert!(
            text.contains(expected),
            "expected behavior panel to contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn behavior_panel_compacts_large_analytics_counts() {
    let mut payload = sample_behavior_payload();
    payload.activity.breakdown[0].total_tokens = 18_214_785_227;
    payload.tools.breakdown[0].calls = 137_075;

    let text = render_behavior_text(payload, 160, 40);
    for expected in ["tokens=18.2B", "calls=137.1K"] {
        assert!(
            text.contains(expected),
            "behavior panel should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn behavior_panel_renders_no_data_degraded_and_compare_warnings() {
    let mut payload = sample_behavior_payload();
    payload.activity = ActivityPayload {
        support: support(
            false,
            "no_data",
            Some("No normalized behavior facts match this filter."),
        ),
        breakdown: Vec::new(),
    };
    payload.tools = ToolsPayload {
        support: support(false, "degraded", Some("Tool-level evidence unavailable.")),
        breakdown: Vec::new(),
    };
    payload.optimize = OptimizePayload {
        support: support(
            false,
            "no_data",
            Some("No behavior facts for optimization."),
        ),
        score: 100,
        grade: "A".to_string(),
        estimated_savings_tokens: 0,
        estimated_savings_usd: 0.0,
        findings: Vec::new(),
    };
    payload.compare = ModelComparePayload {
        support: support(
            false,
            "insufficient_models",
            Some("At least two models with local usage are required for comparison."),
        ),
        candidates: vec![CompareModelCandidate {
            model: "gpt-5".to_string(),
            calls: 1,
            turns: 1,
            edit_turns: 0,
            total_tokens: 1_000,
            estimated_cost_usd: 0.05,
            low_sample: true,
        }],
        model_a: None,
        model_b: None,
        metrics: Vec::new(),
        category_head_to_head: Vec::new(),
        working_style: Vec::new(),
        warning: Some("Need at least two models in the current filter.".to_string()),
    };

    let text = render_behavior_text(payload, 160, 40);

    for expected in [
        "no-data",
        "degraded",
        "insufficient-models",
        "score and savings are not calculated",
        "At least two",
    ] {
        assert!(
            text.contains(expected),
            "expected degraded behavior panel to contain '{expected}', got: {text}"
        );
    }
    assert!(
        !text.contains("Score 100"),
        "unsupported optimize state must not present no-data as a perfect score: {text}"
    );
}
