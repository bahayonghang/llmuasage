# Baseline integration test inventory

Captured before any product/test-graph edit with `cargo test --locked --all-features -- --list` and per-target `--test <target> -- --list` probes.

- Library unit tests: 797
- Integration tests: 202
- Total tests: 999
- Root integration targets: 14

The move-only gate compares the following 202 leaf names as a multiset. Target/module prefixes may change, but every leaf must remain exactly once.

## architecture_dependencies (3)

- `fixtures_cover_supported_rust_path_forms`
- `remote_layer_does_not_depend_on_commands`
- `sync_layer_does_not_depend_on_commands`

## hour_of_week (2)

- `hour_of_week_folds_both_sides_of_a_dst_fallback_into_the_same_local_hour`
- `hour_of_week_zero_fills_and_applies_iana_timezone`

## local_flow (6)

- `cleanup_continues_after_one_integration_fails`
- `legacy_cleanup_handles_all_owned_artifacts_and_is_idempotent`
- `local_flow_bootstraps_and_syncs_without_installing_integrations`
- `sync_prices_claude_fable_and_mythos_usage`
- `sync_prices_gpt_5_6_per_request_for_codex_and_opencode`
- `wrapper_only_cleanup_is_audited_once_and_then_becomes_a_noop`

## logs_session_analytics (1)

- `logs_support_session_and_single_event_detail`

## m2_raw_archive_logs (15)

- `cancel_within_1500ms`
- `cancel_within_1500ms_with_5_pending_files`
- `file_boundary_cancel_preserves_written_events`
- `human_sync_subprocess_stderr_contains_no_ansi_escapes`
- `json_events_subprocess_emits_ndjson_per_event`
- `logs_cursor_round_trip`
- `opencode_row_serialized_as_json_in_raw_table`
- `raw_archive_off_by_default`
- `raw_archive_opt_in_is_returned_by_logs`
- `recent_ready_emitted_per_source_when_recent_days_set`
- `recent_window_filters_old_events_without_advancing_full_history_cursor`
- `reset_for_source_codex_keeps_claude_intact`
- `reset_usage_data_clears_behavior_facts`
- `source_filtered_sync_keeps_other_sources_intact`
- `start_run_complete_lifecycle_observable_via_snapshot`

## public_api (3)

- `cli_sync_uses_shared_stable_validation_codes`
- `root_facade_exposes_sync_job_types`
- `root_facade_opens_store_and_dashboard`

## remote_lifecycle (5)

- `json_events_emit_remote_host_started_finished_and_skipped`
- `source_status_reports_three_host_states_and_omits_live`
- `unreachable_remote_does_not_fail_sync_or_sweep_source_files`
- `unreachable_remote_missing_files_do_not_block_automatic_repair`
- `unreachable_remote_missing_files_do_not_block_rebuild`

## remote_shard_transport (1)

- `emit_shards_cli_leaves_user_db_counts_and_lock_unchanged`

## report_commands (24)

- `catalog_cli_applies_reports_and_resets_overlay_across_processes`
- `cli_home_flag_overrides_llmusage_home_env`
- `cli_reports_use_camel_case_without_changing_other_json_surfaces`
- `daily_defaults_to_last_7_days_and_all_restores_history`
- `daily_human_output_uses_aggregate_ccusage_style_columns_and_no_default_info_logs`
- `diagnostics_includes_logs_summary_without_dumping_entries`
- `doctor_refresh_pricing_accepts_native_litellm_snapshot`
- `doctor_refresh_pricing_writes_catalog_version_meta`
- `focused_source_reports_match_source_filters_without_comparison_fields`
- `host_filter_applies_to_activity_and_tools`
- `host_filter_keeps_totals_consistent_and_lists_unknown_labels`
- `logging_runtime_writes_ndjson_file`
- `logs_command_filters_level_and_command`
- `no_cost_projects_all_report_output_without_changing_tokens`
- `report_commands_emit_unified_camel_case_json_from_sqlite`
- `report_commands_use_persisted_cost_columns`
- `report_date_filters_accept_iso_and_compact_forms_equivalently`
- `report_help_and_legacy_help_still_parse`
- `report_stdout_is_not_polluted_by_logging`
- `run_tracked_records_failure_for_sync_rebuild`
- `sections_output_keeps_current_period_first_and_flattens_json`
- `source_status_command_executes_against_fresh_runtime`
- `statusline_outputs_single_line_without_stdin`
- `weekly_command_uses_monday_periods_and_shared_agent_json`

## source_file_state (2)

- `deleted_then_seen_again_resurrects_to_live`
- `three_entries_lead_to_consistent_state`

## sync_regression (84)

- `antigravity_append_replays_file_and_replaces_stale_rows`
- `antigravity_cli_home_override`
- `antigravity_deleted_conversation_preserves_history`
- `antigravity_missing_root_reports_no_data`
- `antigravity_recent_days_run_skips_reset_and_window_filters`
- `antigravity_sync_twice_is_idempotent`
- `antigravity_tokens_separate_output_from_reasoning`
- `antigravity_upgrade_from_historical_only_keeps_legacy_rows`
- `bootstrap_migrates_legacy_usage_event_before_session_index`
- `claude_changed_project_does_not_replay_other_projects`
- `codex_append_scans_only_changed_file`
- `codex_lossy_rebuild_can_be_explicitly_allowed`
- `codex_missing_history_survives_regular_sync_and_blocks_rebuild_by_default`
- `default_ccr_provider_map_labels_sync_and_rebuild`
- `doctor_warns_on_recovered_aborted_runs`
- `dsh_append_imports_new_frame_after_reparse`
- `dsh_deleted_session_preserves_history`
- `dsh_family_replay_keeps_shared_event_when_owner_rewrites`
- `dsh_first_sync_marks_current_token_accounting`
- `dsh_fork_parent_and_child_do_not_double_count`
- `dsh_home_override_points_parser_at_custom_root`
- `dsh_missing_root_reports_no_data`
- `dsh_parser_provider_survives_loaded_ccr_timeline`
- `dsh_recent_days_run_skips_reset_and_does_not_advance_cursor`
- `dsh_rewrite_replaces_stale_rows`
- `dsh_sync_twice_is_idempotent`
- `export_failure_marks_run_failed_immediately`
- `grok_home_override_is_honored`
- `grok_legacy_marker_2_replays_on_unbounded_sync`
- `grok_missing_root_reports_passive_no_data`
- `grok_session_replay_converges_and_protects_missing_sidecars`
- `grok_turn_usage_is_precise_idempotent_and_replays`
- `historical_hook_rows_and_holder_kind_remain_read_compatible`
- `hot_sync_keeps_unchanged_source_files_live_and_reports_stored_events`
- `kimi_append_imports_only_new_record`
- `kimi_code_home_override_and_raw_models_survive_query_layer`
- `kimi_deleted_history_survives_regular_sync_and_blocks_rebuild`
- `kimi_first_sync_imports_only_turn_usage_with_raw_model`
- `kimi_first_sync_marks_current_token_accounting`
- `kimi_missing_root_sync_succeeds_and_status_tracks_passive_data`
- `kimi_rewrite_resets_and_replaces_old_rows`
- `kimi_sync_twice_is_idempotent`
- `omp_behavior_facts_persist_turns_and_tool_calls`
- `omp_recent_cutoff_does_not_write_orphan_behavior_facts`
- `omp_rewrite_clears_old_path_hash_behavior_facts`
- `omp_source_reported_cost_survives_recompute`
- `omp_source_sync_refuses_until_pi_split_migration`
- `omp_stamps_provider_and_project_dimensions`
- `omp_syncs_default_root_and_projects_status`
- `opencode_channel_db_without_opencode_home_is_imported`
- `opencode_explicit_db_env_is_imported`
- `opencode_high_water_handles_same_timestamp_ids`
- `opencode_missing_db_reports_absent_without_failing_sync`
- `opencode_part_scan_uses_persisted_high_water`
- `opencode_replaced_db_resets_high_water`
- `pi_agent_dir_lists_multiple_roots_and_dedupes_canonical_files`
- `pi_combines_default_roots_and_preserves_usage_across_query`
- `pi_repeat_append_and_rewrite_follow_file_cursor_contract`
- `pi_token_accounting_bump_replays_omp_identity_set`
- `rebuild_rejects_unattributed_antigravity_history_and_preserves_rows`
- `source_breakdown_matches_bucket_totals`
- `source_sync_stats_absent_wire_contract_is_backward_compatible`
- `sqlite_worker_lock_is_exclusive`
- `status_renders_lock_holder`
- `sync_failure_from_invalid_active_pricing_snapshot_marks_run_failed`
- `sync_failure_marks_run_failed_immediately`
- `sync_hot_run_and_append_remain_incremental`
- `sync_replay_replaces_old_file_totals`
- `sync_summary_table_is_stdout_only_without_ansi_or_completion_sentence`
- `v0_db_with_worker_lease_table_rename_to_worker_lock_succeeds`
- `worker_lock_heartbeat_refreshes_existing_lease`
- `zcode_append_imports_only_new_rows`
- `zcode_cancel_after_first_page_does_not_advance_skip_watermark`
- `zcode_db_rebuild_replays_from_zero`
- `zcode_first_sync_marks_current_token_accounting`
- `zcode_home_override_points_parser_at_custom_root`
- `zcode_late_completing_request_is_not_missed`
- `zcode_missing_root_sync_succeeds_and_reports_no_data`
- `zcode_new_unfinished_row_after_skip_watermark_reports_once`
- `zcode_rebuild_resets_skip_watermark`
- `zcode_recent_days_does_not_advance_skip_watermark`
- `zcode_recent_days_run_filters_window_without_advancing_cursor`
- `zcode_skips_error_and_cancelled_rows_and_counts_them`
- `zcode_sync_twice_is_idempotent`

## token_accounting_parity (17)

- `automatic_repair_cancellation_does_not_finish_or_advance_marker`
- `automatic_repair_handles_multiple_legacy_sources_in_registry_order`
- `automatic_repair_never_uses_lossy_opt_in_from_normal_sync_options`
- `automatic_repair_parser_failure_does_not_finish_or_advance_marker`
- `automatic_repair_preflights_all_legacy_sources_before_any_reset`
- `automatic_repair_resets_only_legacy_sources_in_a_mixed_run`
- `bounded_sync_refuses_legacy_repair_before_resetting_history`
- `ccusage_token_semantics_are_consistent_across_sources_and_queries`
- `empty_and_parserless_selected_sources_do_not_enter_automatic_repair`
- `full_rebuild_checks_all_parser_risks_before_resetting_any_source`
- `full_rebuild_refused_while_unattributed_antigravity_history_exists`
- `ordinary_sync_automatically_repairs_safe_legacy_source`
- `serve_repair_propagates_safe_rebuild_failures`
- `serve_repair_rebuilds_multiple_legacy_sources_in_registry_order`
- `serve_repair_rebuilds_safe_legacy_sources_and_unblocks_normal_sync`
- `serve_repair_skips_lossy_legacy_source_without_deleting_history`
- `targeted_current_sync_ignores_unselected_legacy_source`

## tui_panels_prop (35)

- `behavior_panel_compacts_large_analytics_counts`
- `behavior_panel_renders_all_behavior_sections_and_sample_rows`
- `behavior_panel_renders_no_data_degraded_and_compare_warnings`
- `daily_panel_renders_tokscale_style_token_channels`
- `daily_panel_uses_compact_columns_on_narrow_widths`
- `dashboard_shell_renders_tokscale_style_header_and_footer`
- `dashboard_shell_uses_short_labels_on_narrow_widths`
- `hourly_panel_renders_tokscale_table_and_day_separators`
- `models_narrow_and_very_narrow_drop_identity_columns`
- `models_nocolor_has_no_styles`
- `models_visible_window_matches_full_dataset_buffer`
- `models_wide_headers_default_cost_sort_and_no_long_tail_fold`
- `models_wide_paints_inferred_provider_and_joined_sources`
- `monthly_visible_window_matches_full_dataset_buffer`
- `nav_bar_renders_agents_panel_shortcut`
- `overview_panel_compacts_screenshot_scale_statistics_in_wide_and_narrow_layouts`
- `overview_panel_empty_models_and_no_long_tail`
- `overview_panel_nocolor_has_no_styles`
- `overview_panel_renders_chart_and_cost_list`
- `prop_model_table_renders_all_required_columns`
- `prop_overview_panel_renders_all_required_fields`
- `source_picker_overlay_lists_monitor_only_platforms`
- `stats_panel_day_breakdown_and_empty_day`
- `stats_panel_enter_today_and_esc_close`
- `stats_panel_favorite_na_and_context_na`
- `stats_panel_nocolor_has_no_styles`
- `stats_panel_renders_year_calendar_and_two_column_stats`
- `stats_panel_uses_narrow_labels`
- `stats_panel_uses_wide_labels_at_80`
- `usage_overlay_keeps_rebuild_protection_facts_neutral`
- `usage_overlay_renders_sync_status_and_platform_monitor_summary`
- `usage_overlay_uses_compact_columns_on_narrow_widths`
- `usage_panel_can_reveal_emails`
- `usage_panel_empty_state`
- `usage_panel_renders_quota_accounts_and_hides_emails`

## web_sessions_endpoint (4)

- `duration_sort_does_not_drop_active_sessions_outside_a_span_prefilter`
- `top_sessions_filters_and_sorts_stably`
- `top_sessions_is_empty_for_an_empty_store`
- `top_sessions_keeps_complete_serialized_row_and_identity_fallbacks`
