use crate::{
    fixtures::{FORMULAS, Formula},
    math::{Renderer, SCALE},
};
use std::{error::Error, fs, hint::black_box, path::Path, time::Instant};
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

const REPEATS: usize = 21;

/// The converter is timed with layout and rasterization. In typst-only mode it
/// simply copies the hand-written fixture; only mitex mode measures LaTeX input.
pub fn run(
    mode: &str,
    convert: impl Fn(&Formula) -> Result<String, String>,
) -> Result<(), Box<dyn Error>> {
    run_with_prelude(mode, "", convert)
}

pub fn run_with_prelude(
    mode: &str,
    prelude: &'static str,
    convert: impl Fn(&Formula) -> Result<String, String>,
) -> Result<(), Box<dyn Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("artifacts/{mode}"));
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    let startup = Instant::now();
    let mut renderer = Renderer::with_prelude(prelude);
    let init_ms = startup.elapsed().as_secs_f64() * 1000.0;
    let mut csv = String::from(
        "id,name,style,width_px,height_px,first_ms,uncached_median_ms,uncached_p95_ms,warm_median_ms,warm_p95_ms\n",
    );
    let mut sources = String::new();
    let mut failures = Vec::new();
    let mut images = Vec::new();
    let mut inline_images = Vec::new();
    println!("{mode}: renderer init {init_ms:.3} ms; {REPEATS} repeats; 22pt at {SCALE}x");

    for (index, formula) in FORMULAS.iter().enumerate() {
        for display in [true, false] {
            let style = if display { "display" } else { "inline" };
            typst::comemo::evict(0);
            let measure = |renderer: &mut Renderer| -> Result<(Pixmap, f64, String), String> {
                let start = Instant::now();
                let body = convert(formula)?;
                let image = renderer.render_math(&body, display, SCALE)?;
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                Ok((image, ms, body))
            };
            let (image, first, body) = match measure(&mut renderer) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!(
                        "{:02} {} ({style}): {error}",
                        index + 1,
                        formula.name
                    ));
                    continue;
                }
            };
            let mut uncached = Vec::new();
            let mut warm = Vec::new();
            for _ in 0..REPEATS {
                typst::comemo::evict(0);
                let (pixmap, ms, _) = measure(&mut renderer)?;
                black_box(pixmap);
                uncached.push(ms);
            }
            for _ in 0..REPEATS {
                let (pixmap, ms, _) = measure(&mut renderer)?;
                black_box(pixmap);
                warm.push(ms);
            }
            let (median, p95) = percentiles(&mut uncached);
            let (warm_median, warm_p95) = percentiles(&mut warm);
            csv.push_str(&format!("{},{},{style},{},{},{first:.6},{median:.6},{p95:.6},{warm_median:.6},{warm_p95:.6}\n", index + 1, formula.name, image.width(), image.height()));
            sources.push_str(&format!(
                "## {:02} {} ({style})\n\nLaTeX: `{}`\n\nTypst body: `{body}`\n\n",
                index + 1,
                formula.name,
                formula.latex
            ));
            image.save_png(out.join(format!("{:02}-{style}.png", index + 1)))?;
            println!(
                "{:02} {style:7} {:25} {:4}x{:<3} uncached {median:7.3} ms; warm {warm_median:7.3} ms",
                index + 1,
                formula.name,
                image.width(),
                image.height()
            );
            if display {
                images.push((index + 1, formula.name, image));
            } else {
                inline_images.push((index + 1, formula.name, image));
            }
        }
    }
    fs::write(out.join("timings.csv"), csv)?;
    fs::write(out.join("sources.md"), sources)?;
    fs::write(out.join("failures.txt"), failures.join("\n"))?;
    fs::write(
        out.join("run.txt"),
        format!(
            "mode={mode}\narch={}\nos={}\nrenderer_init_ms={init_ms:.6}\nfont_size_pt=22\nscale=2\nrepeats={REPEATS}\nfailed_cases={}\nbinary_bytes={}\n\nUncached = comemo::evict(0) before each sample; font/library initialization excluded.\nWarm = same formula again, Typst internal memoization retained; rasterization still runs.\nPNG encoding, output I/O and gallery assembly excluded. No application pixmap cache.\nFirst is per-case first after eviction; only the first case includes process-first glyph work.\nTypst-only uses manual Typst counterparts, not LaTeX conversion.\n",
            std::env::consts::ARCH,
            std::env::consts::OS,
            failures.len(),
            fs::metadata(std::env::current_exe()?)?.len()
        ),
    )?;
    for (page, entries) in images.chunks(10).enumerate() {
        gallery(&mut renderer, entries, mode, "display")?
            .save_png(out.join(format!("gallery-{}.png", page + 1)))?;
    }
    for (page, entries) in inline_images.chunks(10).enumerate() {
        gallery(&mut renderer, entries, mode, "inline")?
            .save_png(out.join(format!("inline-gallery-{}.png", page + 1)))?;
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} failed cases; see {}",
            failures.len(),
            out.join("failures.txt").display()
        )
        .into())
    }
}

fn percentiles(samples: &mut [f64]) -> (f64, f64) {
    samples.sort_by(f64::total_cmp);
    (
        samples[samples.len() / 2],
        samples[(samples.len() as f64 * 0.95).ceil() as usize - 1],
    )
}

fn gallery(
    renderer: &mut Renderer,
    entries: &[(usize, &str, Pixmap)],
    mode: &str,
    style: &str,
) -> Result<Pixmap, String> {
    let mut canvas = Pixmap::new(2000, 2000).unwrap();
    canvas.fill(Color::from_rgba8(21, 21, 21, 255));
    let title = renderer.render_source(format!(
        "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n#set text(font: \"Libertinus Serif\", size: 23pt, fill: rgb(\"e8e6e3\"))\nMathstral / {mode} / {style} samples"), SCALE)?;
    canvas.draw_pixmap(
        64,
        48,
        title.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    for (slot, (id, name, image)) in entries.iter().enumerate() {
        let x = 64 + (slot % 2) as i32 * 980;
        let y = 170 + (slot / 2) as i32 * 360;
        let label = renderer.render_source(format!(
            "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n#set text(font: \"Libertinus Serif\", size: 14pt, fill: rgb(\"999999\"))\n{id:02} / {name}"), SCALE)?;
        if image.width() > 880 || image.height() > 270 {
            return Err(format!(
                "gallery cell too small for {name}; individual PNG retained"
            ));
        }
        canvas.draw_pixmap(
            x,
            y,
            label.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        canvas.draw_pixmap(
            x,
            y + 60,
            image.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    Ok(canvas)
}
