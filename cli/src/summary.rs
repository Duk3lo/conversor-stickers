use std::fmt::Write as _;
use std::path::Path;

use conversor_core::{
    Canvas, ConversionOptions, Preset, Report, TimePlan, decimal_1, readable_size,
};

pub(crate) fn plan(
    plan: &TimePlan,
    options: &ConversionOptions,
    output: &Path,
    preview: Option<&Path>,
) -> String {
    let preset = options.preset();
    let fps = options.fps();
    let mut text = String::from("Plan (--dry-run: no se ha escrito nada)\n");
    line(&mut text, "Preset", preset.name);
    line(
        &mut text,
        "Tramo",
        &format!(
            "{} s → {} s del video ({} s)",
            decimal_1(plan.start_s),
            decimal_1(plan.start_s + plan.input_duration_s),
            decimal_1(plan.input_duration_s)
        ),
    );
    line(&mut text, "Velocidad", &speed(plan.factor_speed));
    line(
        &mut text,
        "Duración",
        &format!("{} s", decimal_1(plan.output_duration_s)),
    );
    line(
        &mut text,
        "fps",
        &format!(
            "{} ({} frames antes de quitar duplicados)",
            fps.value(),
            plan.frame_count(fps)
        ),
    );
    line(&mut text, "Lienzo", &preset_canvas(preset));
    line(
        &mut text,
        "Peso máximo",
        &readable_size(options.max_size().bytes()),
    );
    line(&mut text, "Salida", &output.display().to_string());
    if let Some(path) = preview {
        line(&mut text, "Vista previa", &path.display().to_string());
    }
    text
}

pub(crate) fn report(report: &Report, output: &Path, preview: Option<&Path>) -> String {
    let mut text = format!("Creado {}\n", output.display());
    line(
        &mut text,
        "Peso",
        &format!(
            "{} (máximo {})",
            readable_size(report.size_bytes()),
            readable_size(report.max_size.bytes())
        ),
    );
    line(
        &mut text,
        "Duración",
        &format!("{} s", decimal_1(report.duration_ms as f64 / 1000.0)),
    );
    line(&mut text, "Velocidad", &speed(report.plan.factor_speed));
    line(&mut text, "fps", &report.attempt.fps.value().to_string());
    let mut canvas = format!("{0}x{0}", report.canvas_side);
    if report.content_side != report.canvas_side {
        let _ = write!(
            canvas,
            " (contenido {0}x{0} con borde transparente)",
            report.content_side
        );
    }
    line(&mut text, "Lienzo", &canvas);
    let reductions = if report.reductions.is_empty() {
        "sin reducciones".to_string()
    } else {
        report
            .reductions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    line(&mut text, "Reducciones", &reductions);
    if let Some(path) = preview {
        line(&mut text, "Vista previa", &path.display().to_string());
    }
    text
}

fn line(text: &mut String, tag: &str, value: &str) {
    let _ = writeln!(text, "  {:<13} {value}", format!("{tag}:"));
}

fn preset_canvas(preset: &Preset) -> String {
    let side = preset.canvas.side();
    match preset.canvas {
        Canvas::Exact(_) => format!("{side}x{side}"),
        Canvas::Max(_) => format!("hasta {side}x{side}"),
    }
}

fn speed(factor: f64) -> String {
    if factor == 1.0 {
        "1x".to_string()
    } else {
        format!("{}x (--fit)", decimals(factor, 2))
    }
}

fn decimals(value: f64, digits: usize) -> String {
    let text = format!("{value:.digits$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    text.replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_carry_comma_and_without_zeros_of_more() {
        assert_eq!(decimals(1.5, 2), "1,5");
        assert_eq!(decimals(2.0, 2), "2");
        assert_eq!(decimals(4.0 / 3.0, 2), "1,33");
        assert_eq!(speed(1.0), "1x");
        assert_eq!(speed(1.8), "1,8x (--fit)");
    }
}
