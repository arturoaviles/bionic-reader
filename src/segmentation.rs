//! Unicode word segmentation and reusable bionic-reading emphasis spans.
//!
//! [`UnicodeSegmenter`] follows Unicode Standard Annex #29 and uses ICU's
//! automatic complex-script data. [`FixationEngine`] applies the configured
//! word frequency and emphasis length to word ranges from any [`WordSegmenter`].

use std::fmt;
use std::ops::Range;
use std::str;

use icu_segmenter::WordSegmenter as IcuWordSegmenter;
use icu_segmenter::WordSegmenterBorrowed;
use icu_segmenter::options::WordBreakInvariantOptions;
use unicode_segmentation::UnicodeSegmentation;

use crate::settings::{LengthRule, Settings};

/// Produces sorted, non-overlapping byte ranges for word-like text.
///
/// Implementations must return ranges within `text` whose endpoints are UTF-8
/// and extended-grapheme-cluster boundaries. Ranges may omit punctuation,
/// numbers, and other text that should not receive emphasis. Implementations
/// also satisfy `Debug + Send + Sync` so they can be retained by a [`Transformer`].
///
/// [`Transformer`]: crate::transformer::Transformer
pub trait WordSegmenter: fmt::Debug + Send + Sync {
    /// Returns byte ranges for word-like parts of `text`.
    #[must_use]
    fn word_ranges(&self, text: &str) -> Vec<Range<usize>>;

    /// Returns the start of a trailing suffix that may still continue a word.
    ///
    /// Incremental renderers use this to keep a possible trailing word buffered
    /// until a later boundary can establish its complete range. The offset must
    /// be a UTF-8 and grapheme boundary. It may identify a dangling connector
    /// suffix that [`word_ranges`](Self::word_ranges) does not include yet. The
    /// default returns the final word range when it reaches the end of `text`;
    /// custom segmenters can override it for their streaming rules.
    #[must_use]
    fn trailing_word_start(&self, text: &str) -> Option<usize> {
        self.word_ranges(text)
            .into_iter()
            .last()
            .filter(|range| range.start < range.end && range.end == text.len())
            .map(|range| range.start)
    }

    /// Returns the suffix that a terminal stream should retain between chunks.
    ///
    /// Unlike [`trailing_word_start`](Self::trailing_word_start), this boundary
    /// can favor prompt output when punctuation has already closed a word. The
    /// default follows the segmenter's regular trailing-word boundary.
    #[must_use]
    fn terminal_trailing_word_start(&self, text: &str) -> Option<usize> {
        self.trailing_word_start(text)
    }
}

/// ICU-backed Unicode word segmentation with dictionary support for complex scripts.
pub struct UnicodeSegmenter {
    segmenter: WordSegmenterBorrowed<'static>,
}

impl UnicodeSegmenter {
    /// Creates a segmenter using ICU's automatic Unicode word-break data.
    #[must_use]
    pub fn new() -> Self {
        Self {
            segmenter: IcuWordSegmenter::new_auto(WordBreakInvariantOptions::default()),
        }
    }
}

impl Default for UnicodeSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for UnicodeSegmenter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UnicodeSegmenter")
    }
}

impl WordSegmenter for UnicodeSegmenter {
    fn word_ranges(&self, text: &str) -> Vec<Range<usize>> {
        let grapheme_boundaries: Vec<usize> = text
            .grapheme_indices(true)
            .map(|(start, _)| start)
            .chain(std::iter::once(text.len()))
            .collect();
        let mut ranges: Vec<Range<usize>> = Vec::new();
        // ICU segmentation always begins with the start-of-text boundary at zero.
        let mut start = 0;
        for end in self.segmenter.segment_str(text).skip(1) {
            let range = start..end;
            if contains_letter_grapheme(&text[range]) {
                let aligned = grapheme_floor(&grapheme_boundaries, start)
                    ..grapheme_ceiling(&grapheme_boundaries, end);
                if let Some(previous) = ranges.last_mut() {
                    if aligned.start < previous.end {
                        previous.end = previous.end.max(aligned.end);
                        start = end;
                        continue;
                    }
                }
                ranges.push(aligned);
            }
            start = end;
        }

        ranges
    }

    fn trailing_word_start(&self, text: &str) -> Option<usize> {
        let final_range = self
            .word_ranges(text)
            .into_iter()
            .last()
            .filter(|range| range.end == text.len())
            .map(|range| range.start);
        final_range
            .into_iter()
            .chain(trailing_connector_candidate_start(text))
            .chain(trailing_non_whitespace_start(text))
            .min()
    }

    fn terminal_trailing_word_start(&self, text: &str) -> Option<usize> {
        let final_range = self
            .word_ranges(text)
            .into_iter()
            .last()
            .filter(|range| range.end == text.len())
            .map(|range| range.start);
        final_range
            .into_iter()
            .chain(trailing_connector_candidate_start(text))
            .chain(trailing_prepend_start(text))
            .min()
    }
}

pub(crate) fn is_word_connector(character: char) -> bool {
    matches!(
        character,
        '\u{005f}' // ExtendNumLet: underscore
            | '\u{007e}' // ICU can join this before an alphabetic segment
            | '\u{003a}' // MidLetter: colon
            | '\u{00b7}'
            | '\u{0387}'
            | '\u{05f4}'
            | '\u{2027}'
            | '\u{fe13}'
            | '\u{fe14}'
            | '\u{ff1a}'
            | '\u{002e}' // MidNumLet: full stop
            | '\u{2024}'
            | '\u{fe52}'
            | '\u{ff0e}'
            | '\u{002c}' // MidNum
            | '\u{003b}'
            | '\u{037e}'
            | '\u{0589}'
            | '\u{060c}'
            | '\u{066c}'
            | '\u{07f8}'
            | '\u{2044}'
            | '\u{fe10}'
            | '\u{fe50}'
            | '\u{ff0c}'
            | '\u{ff1b}'
    )
}

fn trailing_connector_candidate_start(text: &str) -> Option<usize> {
    let graphemes: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    let last = graphemes.len().checked_sub(1)?;
    if !is_connector_grapheme(graphemes[last].1) {
        return None;
    }

    let mut cursor = last;
    while cursor > 0
        && (is_word_grapheme(graphemes[cursor - 1].1)
            || is_connector_grapheme(graphemes[cursor - 1].1))
    {
        cursor -= 1;
    }
    Some(graphemes[cursor].0)
}

fn trailing_non_whitespace_start(text: &str) -> Option<usize> {
    let mut suffix_start = None;
    for (start, grapheme) in text.grapheme_indices(true) {
        if grapheme.chars().any(char::is_whitespace) {
            suffix_start = None;
        } else {
            suffix_start.get_or_insert(start);
        }
    }
    suffix_start
}

fn trailing_prepend_start(text: &str) -> Option<usize> {
    let (start, grapheme) = text.grapheme_indices(true).next_back()?;
    let first = grapheme.chars().next()?;
    is_grapheme_prepend(first).then_some(start)
}

fn is_grapheme_prepend(character: char) -> bool {
    matches!(
        character as u32,
        0x0600..=0x0605
            | 0x06dd
            | 0x070f
            | 0x0890..=0x0891
            | 0x08e2
            | 0x0d4e
            | 0x110bd
            | 0x110cd
            | 0x111c2..=0x111c3
            | 0x1193f
            | 0x11941
            | 0x11a84..=0x11a89
            | 0x11d46
            | 0x11f02
    )
}

fn is_word_grapheme(grapheme: &str) -> bool {
    grapheme
        .chars()
        .any(|character| character.is_alphabetic() || character.is_numeric())
}

fn is_connector_grapheme(grapheme: &str) -> bool {
    grapheme
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '\'' | '’' | 'ʼ') || is_word_connector(first))
}

fn grapheme_floor(boundaries: &[usize], offset: usize) -> usize {
    let index = boundaries.partition_point(|boundary| *boundary <= offset) - 1;
    boundaries[index]
}

fn grapheme_ceiling(boundaries: &[usize], offset: usize) -> usize {
    boundaries[boundaries.partition_point(|boundary| *boundary < offset)]
}

/// Selects Unicode letter prefixes according to a settings snapshot.
///
/// The engine counts eligible letter words across calls. Reuse one engine for
/// sequential text fragments when selection frequency should continue across
/// those fragments. The engine owns only the fixation-related settings; later
/// changes to the source [`Settings`] do not affect it.
#[derive(Clone, Debug)]
#[must_use = "use the fixation engine to select emphasis spans"]
pub struct FixationEngine {
    settings: FixationSettings,
    eligible_seen: usize,
}

#[derive(Clone, Debug)]
struct FixationSettings {
    fraction: f64,
    every: usize,
    min_word_length: usize,
    length_rules: Vec<LengthRule>,
}

impl FixationSettings {
    fn fraction_for_length(&self, length: usize) -> f64 {
        self.length_rules
            .iter()
            .find(|rule| length <= rule.max_length)
            .map_or(self.fraction, |rule| rule.fraction)
    }
}

impl FixationEngine {
    /// Creates an engine with a validated snapshot of the fixation settings.
    ///
    /// # Panics
    ///
    /// Panics immediately if `settings` is invalid. Validate untrusted settings
    /// with [`Settings::validate`] before constructing the engine.
    pub fn new(settings: &Settings) -> Self {
        settings
            .validate()
            .expect("fixation engine settings must be valid");
        Self {
            settings: FixationSettings {
                fraction: settings.fraction,
                every: settings.every,
                min_word_length: settings.min_word_length,
                length_rules: settings.length_rules.clone(),
            },
            eligible_seen: 0,
        }
    }

    /// Selects emphasis byte ranges in `text` using `segmenter`'s word ranges.
    ///
    /// Each eligible word advances the configured frequency, even when its
    /// selected fraction is zero. Letter counts use extended grapheme clusters,
    /// so combining accents and joined emoji sequences are never split.
    ///
    /// # Panics
    ///
    /// Panics if `segmenter` returns ranges that are out of bounds, unordered,
    /// overlapping, or split a UTF-8 character or grapheme cluster.
    #[must_use]
    pub fn spans(&mut self, text: &str, segmenter: &dyn WordSegmenter) -> Vec<Range<usize>> {
        let word_ranges = segmenter.word_ranges(text);
        validate_word_ranges(text, &word_ranges);

        let mut spans = Vec::new();
        for word_range in word_ranges {
            for letter_run in letter_runs(text, word_range) {
                let letter_count = letter_run.letters.len();
                if letter_count < self.settings.min_word_length {
                    continue;
                }

                self.eligible_seen = self.eligible_seen.saturating_add(1);
                if (self.eligible_seen - 1) % self.settings.every != 0 {
                    continue;
                }

                let fraction = self.settings.fraction_for_length(letter_count);
                if fraction <= 0.0 {
                    continue;
                }

                // The string is bounded by addressable memory and the validated
                // fraction is within 0..=1, so this rounded count is in range.
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss,
                    reason = "the bounded letter count and validated fraction make this safe"
                )]
                let emphasized_letters = (fraction * letter_count as f64).ceil() as usize;
                // A positive fraction and a nonempty letter run round up to at least one.
                let end = letter_run.letters[emphasized_letters.min(letter_count) - 1].end;
                spans.push(letter_run.start..end);
            }
        }

        spans
    }

    pub(crate) fn advance_ranges_before(
        &mut self,
        text: &str,
        ranges: &[Range<usize>],
        end_before: usize,
    ) {
        assert!(
            end_before <= text.len()
                && text.is_char_boundary(end_before)
                && (end_before == text.len()
                    || text
                        .grapheme_indices(true)
                        .any(|(start, _)| start == end_before)),
            "segment boundary is not a grapheme boundary"
        );
        validate_word_ranges(text, ranges);

        for range in ranges.iter().filter(|range| range.end <= end_before) {
            for letter_run in letter_runs(text, range.clone()) {
                self.advance_if_eligible(letter_run.letters.len());
            }
        }
    }

    pub(crate) fn advance_if_eligible(&mut self, letter_count: usize) {
        if letter_count >= self.settings.min_word_length {
            self.eligible_seen = self.eligible_seen.saturating_add(1);
        }
    }
}

/// Returns emphasis spans for one complete string with default Unicode segmentation.
///
/// # Panics
///
/// Panics immediately if `settings` is invalid. Use [`Settings::validate`] for
/// fallible validation.
#[must_use]
pub fn emphasis_spans(text: &str, settings: &Settings) -> Vec<Range<usize>> {
    let mut engine = FixationEngine::new(settings);
    let segmenter = UnicodeSegmenter::new();
    engine.spans(text, &segmenter)
}

#[derive(Debug)]
struct LetterRun {
    start: usize,
    letters: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
struct GraphemeRange<'a> {
    range: Range<usize>,
    text: &'a str,
}

fn validate_word_ranges(text: &str, ranges: &[Range<usize>]) {
    let boundaries: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(start, _)| start)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut previous_end = 0;

    for range in ranges {
        assert!(
            range.start < range.end && range.end <= text.len(),
            "word segmenter returned an empty or out-of-bounds range"
        );
        assert!(
            text.is_char_boundary(range.start) && text.is_char_boundary(range.end),
            "word segmenter returned a range that splits a UTF-8 character"
        );
        assert!(
            range.start >= previous_end,
            "word segmenter returned unordered or overlapping ranges"
        );
        assert!(
            boundaries.binary_search(&range.start).is_ok()
                && boundaries.binary_search(&range.end).is_ok(),
            "word segmenter returned a range that splits a grapheme cluster"
        );
        previous_end = range.end;
    }
}

fn letter_runs(text: &str, range: Range<usize>) -> Vec<LetterRun> {
    let word = &text[range.clone()];
    let graphemes: Vec<GraphemeRange<'_>> = word
        .grapheme_indices(true)
        .map(|(start, grapheme)| GraphemeRange {
            range: range.start + start..range.start + start + grapheme.len(),
            text: grapheme,
        })
        .collect();
    let mut runs = Vec::new();
    let mut cursor = 0;

    while cursor < graphemes.len() {
        if !is_letter_grapheme(graphemes[cursor].text) {
            cursor += 1;
            continue;
        }

        let start = graphemes[cursor].range.start;
        let mut letters = vec![graphemes[cursor].range.clone()];
        cursor += 1;
        while cursor < graphemes.len() {
            if is_letter_grapheme(graphemes[cursor].text) {
                letters.push(graphemes[cursor].range.clone());
                cursor += 1;
            } else if is_apostrophe(graphemes[cursor].text)
                && cursor + 1 < graphemes.len()
                && is_letter_grapheme(graphemes[cursor + 1].text)
            {
                cursor += 1;
            } else {
                break;
            }
        }
        runs.push(LetterRun { start, letters });
    }

    runs
}

pub(crate) fn is_letter_grapheme(grapheme: &str) -> bool {
    grapheme
        .chars()
        .any(|character| character.is_alphabetic() && !is_grapheme_extension(character))
}

fn contains_letter_grapheme(text: &str) -> bool {
    text.graphemes(true).any(is_letter_grapheme)
}

pub(crate) fn is_grapheme_extension(character: char) -> bool {
    if character.is_ascii() {
        return false;
    }

    let mut probe = [0; 5];
    probe[0] = b'a';
    let encoded_len = character.encode_utf8(&mut probe[1..]).len();
    str::from_utf8(&probe[..=encoded_len])
        .expect("a character encoded as UTF-8 remains valid")
        .graphemes(true)
        .count()
        == 1
}

fn is_apostrophe(grapheme: &str) -> bool {
    matches!(grapheme, "'" | "’" | "ʼ")
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use unicode_segmentation::UnicodeSegmentation;

    use crate::settings::Settings;

    use super::{FixationEngine, UnicodeSegmenter, WordSegmenter, emphasis_spans};

    #[derive(Clone, Copy, Debug)]
    struct WholeTextSegmenter;

    impl WordSegmenter for WholeTextSegmenter {
        fn word_ranges(&self, text: &str) -> Vec<Range<usize>> {
            if text.is_empty() {
                Vec::new()
            } else {
                std::iter::once(0..text.len()).collect()
            }
        }
    }

    #[test]
    fn unicode_segmenter_finds_words_in_complex_scripts() {
        let segmenter = UnicodeSegmenter::new();
        for text in ["你好世界", "日本語", "ภาษาไทย", "مرحبا", "Привет"]
        {
            assert!(!segmenter.word_ranges(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn unicode_trailing_start_keeps_dangling_connectors_with_their_word() {
        let segmenter = UnicodeSegmenter::new();
        assert_eq!(segmenter.trailing_word_start("aa'"), Some(0));
        assert_eq!(segmenter.trailing_word_start("alpha:"), Some(0));
        assert_eq!(segmenter.trailing_word_start("prefix aa'"), Some(7));
        assert_eq!(segmenter.trailing_word_start("alpha "), None);
    }

    #[test]
    fn unicode_trailing_start_keeps_marks_attached_to_a_dangling_connector() {
        let segmenter = UnicodeSegmenter::new();
        assert_eq!(segmenter.trailing_word_start("𐒰'\u{a947}"), Some(0));
    }

    #[test]
    fn unicode_trailing_start_holds_non_whitespace_runs_for_grapheme_context() {
        let segmenter = UnicodeSegmenter::new();
        assert_eq!(segmenter.trailing_word_start(" 𐁐.\u{11d3a}"), Some(1));
        assert_eq!(segmenter.trailing_word_start("\u{0600}"), Some(0));
        assert_eq!(segmenter.trailing_word_start("hello "), None);
        assert_eq!(
            segmenter.terminal_trailing_word_start(" 𐁐.\u{11d3a}"),
            Some(1)
        );
        assert_eq!(segmenter.terminal_trailing_word_start("reading!"), None);
        assert_eq!(segmenter.terminal_trailing_word_start("\u{0600}"), Some(0));
    }

    #[test]
    fn fixation_spans_keep_accents_and_emoji_graphemes_whole() {
        assert_eq!(
            emphasis_spans("café cafe\u{301} 👨‍👩‍👧‍👦", &Settings::default()),
            vec![0..2, 6..8]
        );
    }

    #[test]
    #[expect(
        clippy::unicode_not_nfc,
        reason = "decomposed Hangul Jamo exercises a multi-scalar grapheme"
    )]
    fn alphabetic_marks_need_a_letter_base_and_valid_script_bases_remain_eligible() {
        let segmenter = UnicodeSegmenter::new();
        let settings = Settings {
            fraction: 1.0,
            min_word_length: 1,
            ..Settings::default()
        };

        for punctuation_with_mark in ["/\u{0614}", "/\u{0345}", "\u{0614}"] {
            assert!(
                segmenter.word_ranges(punctuation_with_mark).is_empty(),
                "{punctuation_with_mark:?}"
            );
            assert!(
                emphasis_spans(punctuation_with_mark, &settings).is_empty(),
                "{punctuation_with_mark:?}"
            );
            let mut engine = FixationEngine::new(&settings);
            assert!(
                engine
                    .spans(punctuation_with_mark, &WholeTextSegmenter)
                    .is_empty()
            );
        }

        for word in [
            "\u{0600}ب",
            "א\u{05bc}",
            "क\u{093f}",
            "가",
            "ب\u{0614}",
            "\u{1e4d0}",
        ] {
            let spans = emphasis_spans(word, &settings);
            assert_eq!(spans.len(), 1, "{word:?}");
            assert_eq!(spans[0], 0..word.len(), "{word:?}");
            assert!(word.is_char_boundary(spans[0].end));
            assert!(
                word.grapheme_indices(true)
                    .any(|(start, grapheme)| start == spans[0].start
                        && grapheme.len() == spans[0].end),
                "span splits a grapheme in {word:?}"
            );
        }

        for (word, expected) in [
            ("abc123", &[(0, 3)][..]),
            ("123abc", &[(3, 6)][..]),
            ("abc123def", &[(0, 3), (6, 9)][..]),
        ] {
            let spans = emphasis_spans(word, &settings);
            assert_eq!(spans.len(), expected.len(), "{word:?}");
            for (span, expected) in spans.iter().zip(expected) {
                assert_eq!((span.start, span.end), *expected, "{word:?}");
            }
        }
    }

    #[test]
    fn fixation_preserves_apostrophes_and_separates_numeric_runs() {
        let segmenter = WholeTextSegmenter;
        let mut engine = FixationEngine::new(&Settings::default());
        assert_eq!(
            engine.spans("don't 123abc def123", &segmenter),
            vec![0..2, 9..11, 13..15]
        );
    }

    #[test]
    fn selection_frequency_advances_across_spans_calls() {
        let settings = Settings {
            every: 2,
            ..Settings::default()
        };
        let segmenter = UnicodeSegmenter::new();
        let mut engine = FixationEngine::new(&settings);
        assert_eq!(engine.spans("alpha ", &segmenter), vec![0..3]);
        assert!(engine.spans("beta ", &segmenter).is_empty());
        assert_eq!(engine.spans("gamma", &segmenter), vec![0..3]);
    }
    #[test]
    fn default_segmenter_supports_debug_and_default_stream_boundaries() {
        let segmenter = UnicodeSegmenter::default();
        assert_eq!(format!("{segmenter:?}"), "UnicodeSegmenter");
        assert!(segmenter.word_ranges("").is_empty());
        assert_eq!(WholeTextSegmenter.trailing_word_start("word"), Some(0));
        assert_eq!(
            WholeTextSegmenter.terminal_trailing_word_start("word"),
            Some(0)
        );
        assert_eq!(WholeTextSegmenter.trailing_word_start(""), None);
    }
    #[test]
    fn custom_segmenter_contract_rejects_invalid_ranges_before_rendering() {
        #[derive(Debug)]
        struct InvalidSegmenter(Vec<Range<usize>>);
        impl WordSegmenter for InvalidSegmenter {
            fn word_ranges(&self, _text: &str) -> Vec<Range<usize>> {
                self.0.clone()
            }
        }
        for (text, ranges) in [
            ("word", std::iter::once(0..0).collect()),
            ("word", std::iter::once(0..5).collect()),
            ("é", std::iter::once(0..1).collect()),
            ("a\u{301}", std::iter::once(0..1).collect()),
            ("word", vec![0..3, 2..4]),
        ] {
            let outcome = std::panic::catch_unwind(|| {
                FixationEngine::new(&Settings::new()).spans(text, &InvalidSegmenter(ranges))
            });
            outcome.expect_err("invalid custom segmentation must fail before rendering");
        }
    }
    #[test]
    fn zero_fixation_rules_leave_selected_words_unemphasized() {
        for settings in [
            Settings {
                fraction: 0.0,
                ..Settings::new()
            },
            Settings {
                length_rules: vec![crate::settings::LengthRule {
                    max_length: 10,
                    fraction: 0.0,
                }],
                ..Settings::new()
            },
        ] {
            assert!(
                FixationEngine::new(&settings)
                    .spans("reading words", &UnicodeSegmenter::new())
                    .is_empty()
            );
        }
    }
    #[test]
    fn incremental_fixation_rejects_a_boundary_inside_a_grapheme() {
        let outcome = std::panic::catch_unwind(|| {
            FixationEngine::new(&Settings::new()).advance_ranges_before(
                "a\u{301}",
                std::slice::from_ref(&(0..3)),
                1,
            );
        });
        outcome.expect_err("incremental segmentation must preserve grapheme boundaries");
    }
}
