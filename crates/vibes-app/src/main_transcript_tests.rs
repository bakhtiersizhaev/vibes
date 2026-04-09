    #[test]
    fn rendered_or_default_returns_fallback_for_blank_transcript() {
        assert_eq!(
            rendered_or_default("   \n\t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_returns_fallback_for_crlf_only_transcript() {
        assert_eq!(
            rendered_or_default("\r\n \t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_returns_fallback_for_cr_only_transcript() {
        assert_eq!(
            rendered_or_default("\r   \t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_keeps_non_empty_transcript() {
        assert_eq!(
            rendered_or_default("done transcript".to_owned()),
            "done transcript"
        );
    }

    #[test]
    fn rendered_or_default_preserves_multiline_transcript() {
        let rendered = "step 1\nstep 2\nfinal line".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_crlf_prefixed_non_empty_transcript() {
        let rendered = "\r\nstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_cr_prefixed_non_empty_transcript() {
        let rendered = "\rstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_lf_prefixed_non_empty_transcript() {
        let rendered = "\nstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_tab_prefixed_non_empty_transcript() {
        let rendered = "\tstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_keeps_padded_non_empty_transcript() {
        let rendered = "  done transcript  ".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }
}
