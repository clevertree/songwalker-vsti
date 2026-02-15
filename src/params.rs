use std::sync::Arc;

use nih_plug::prelude::*;

/// Top-level plugin parameters exposed to the DAW for automation.
///
/// Per-slot parameters are managed dynamically by the slot manager;
/// these are the global controls.
#[derive(Params)]
pub struct SongWalkerParams {
    /// Master output volume (dB).
    #[id = "master_vol"]
    pub master_volume: FloatParam,

    /// Master pan (-1 = left, 0 = center, +1 = right).
    #[id = "master_pan"]
    pub master_pan: FloatParam,

    /// Global max polyphony across all slots.
    #[id = "max_voices"]
    pub max_voices: IntParam,

    /// Pitch bend range in semitones.
    #[id = "bend_range"]
    pub pitch_bend_range: IntParam,
}

impl Default for SongWalkerParams {
    fn default() -> Self {
        Self {
            master_volume: FloatParam::new(
                "Master Volume",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-60.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-60.0, 6.0),
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            master_pan: FloatParam::new(
                "Master Pan",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_unit("")
            .with_value_to_string(Arc::new(|v| {
                if v.abs() < 0.01 {
                    "C".to_string()
                } else if v < 0.0 {
                    format!("{:.0}L", -v * 100.0)
                } else {
                    format!("{:.0}R", v * 100.0)
                }
            })),

            max_voices: IntParam::new("Max Voices", 256, IntRange::Linear { min: 8, max: 1024 }),

            pitch_bend_range: IntParam::new(
                "Pitch Bend Range",
                2,
                IntRange::Linear { min: 1, max: 48 },
            )
            .with_unit(" st"),
        }
    }
}

/// Per-slot parameters. Each slot in the rack has its own set.
#[derive(Params)]
pub struct SlotParams {
    /// Slot volume (dB).
    #[id = "slot_vol"]
    pub volume: FloatParam,

    /// Slot pan.
    #[id = "slot_pan"]
    pub pan: FloatParam,

    /// Slot mute.
    #[id = "slot_mute"]
    pub mute: BoolParam,

    /// Slot solo.
    #[id = "slot_solo"]
    pub solo: BoolParam,

    /// MIDI channel filter (0 = all, 1–16 = specific channel).
    #[id = "slot_ch"]
    pub midi_channel: IntParam,

    /// Max polyphony for this slot.
    #[id = "slot_poly"]
    pub polyphony: IntParam,

    // --- Envelope overrides ---
    #[id = "slot_atk"]
    pub attack: FloatParam,

    #[id = "slot_dec"]
    pub decay: FloatParam,

    #[id = "slot_sus"]
    pub sustain: FloatParam,

    #[id = "slot_rel"]
    pub release: FloatParam,

    // --- Filter ---
    #[id = "slot_flt_cut"]
    pub filter_cutoff: FloatParam,

    #[id = "slot_flt_res"]
    pub filter_resonance: FloatParam,
}

impl Default for SlotParams {
    fn default() -> Self {
        Self {
            volume: FloatParam::new(
                "Slot Volume",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-60.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-60.0, 6.0),
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            pan: FloatParam::new("Slot Pan", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 }),

            mute: BoolParam::new("Mute", false),
            solo: BoolParam::new("Solo", false),

            midi_channel: IntParam::new("MIDI Channel", 0, IntRange::Linear { min: 0, max: 16 }),

            polyphony: IntParam::new("Polyphony", 64, IntRange::Linear { min: 1, max: 256 }),

            attack: FloatParam::new(
                "Attack",
                0.01,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s"),

            decay: FloatParam::new(
                "Decay",
                0.1,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s"),

            sustain: FloatParam::new(
                "Sustain",
                0.8,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),

            release: FloatParam::new(
                "Release",
                0.3,
                FloatRange::Skewed {
                    min: 0.001,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s"),

            filter_cutoff: FloatParam::new(
                "Filter Cutoff",
                20000.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz"),

            filter_resonance: FloatParam::new(
                "Filter Resonance",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
        }
    }
}
