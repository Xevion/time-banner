//! Benchmarks split by pipeline stage (template -> parse -> rasterize ->
//! encode), each swept across representative elapsed durations, since PNG
//! cost scales with rendered text length as much as with format.

use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use jiff::Timestamp;
use time_banner_render::raster::{Rasterizer, encode_png};
use time_banner_render::template::{RenderContext, render_template};
use time_banner_render::{OutputForm, OutputFormat, render_time};

/// Fixed reference instant so bench output is stable across runs.
const NOW_EPOCH: i64 = 1_700_000_000;

/// Elapsed offsets (seconds; negative = past) chosen to span the text-length
/// buckets `timeago` actually produces, from "5 seconds ago" to "5 years ago".
const DURATIONS: &[(&str, i64)] = &[
    ("seconds", -5),
    ("minutes", -5 * 60),
    ("hours", -5 * 3600),
    ("days", -5 * 86_400),
    ("years", -5 * 365 * 86_400),
    ("future_hours", 5 * 3600),
];

fn now() -> Timestamp {
    Timestamp::from_second(NOW_EPOCH).unwrap()
}

fn context(offset: i64, form: OutputForm) -> RenderContext {
    let now = now();
    let value = Timestamp::from_second(now.as_second() + offset).unwrap();
    RenderContext {
        value,
        output_form: form,
        output_format: OutputFormat::Svg,
        timezone: None,
        format: None,
        now,
    }
}

fn svg_for(offset: i64, form: OutputForm) -> String {
    render_template(context(offset, form)).expect("template renders")
}

fn bench_template(c: &mut Criterion) {
    let mut group = c.benchmark_group("template");
    for &(label, offset) in DURATIONS {
        group.bench_function(label, |b| {
            b.iter(|| render_template(context(offset, OutputForm::Relative)).unwrap())
        });
    }
    group.bench_function("absolute", |b| {
        b.iter(|| render_template(context(0, OutputForm::Absolute)).unwrap())
    });
    group.finish();
}

fn bench_svg_parse(c: &mut Criterion) {
    let rasterizer = Rasterizer::new();
    let mut group = c.benchmark_group("svg_parse");
    for &(label, offset) in DURATIONS {
        let svg = svg_for(offset, OutputForm::Relative);
        group.throughput(Throughput::Bytes(svg.len() as u64));
        group.bench_function(label, |b| {
            b.iter(|| rasterizer.parse(svg.as_bytes()).unwrap())
        });
    }
    group.finish();
}

fn bench_rasterize(c: &mut Criterion) {
    let rasterizer = Rasterizer::new();
    let mut group = c.benchmark_group("rasterize");
    for &(label, offset) in DURATIONS {
        let svg = svg_for(offset, OutputForm::Relative);
        let tree = rasterizer.parse(svg.as_bytes()).unwrap();
        let size = tree.size().to_int_size();
        group.throughput(Throughput::Elements((size.width() * size.height()) as u64));
        group.bench_function(label, |b| b.iter(|| rasterizer.rasterize(&tree)));
    }
    group.finish();
}

fn bench_encode_png(c: &mut Criterion) {
    let rasterizer = Rasterizer::new();
    let mut group = c.benchmark_group("encode_png");
    for &(label, offset) in DURATIONS {
        let svg = svg_for(offset, OutputForm::Relative);
        let tree = rasterizer.parse(svg.as_bytes()).unwrap();
        let pixmap = rasterizer.rasterize(&tree);
        group.throughput(Throughput::Bytes(
            (pixmap.width() * pixmap.height() * 4) as u64,
        ));
        group.bench_function(label, |b| {
            b.iter_batched(
                || pixmap.clone(),
                |pm| encode_png(pm).unwrap(),
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_end_to_end_case(
    group: &mut criterion::BenchmarkGroup<criterion::measurement::WallTime>,
    id: String,
    value: Timestamp,
    now: Timestamp,
    form: OutputForm,
    format: OutputFormat,
) {
    let bytes = render_time(value, now, form, format.clone()).unwrap();
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    group.bench_function(id, |b| {
        b.iter(|| render_time(value, now, form, format.clone()).unwrap())
    });
}

/// End-to-end pipeline (template -> parse -> rasterize -> encode), the
/// number that actually matters for request latency.
fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");

    for &(label, offset) in DURATIONS {
        let ctx = context(offset, OutputForm::Relative);
        bench_end_to_end_case(
            &mut group,
            format!("{label}_svg"),
            ctx.value,
            ctx.now,
            OutputForm::Relative,
            OutputFormat::Svg,
        );
        bench_end_to_end_case(
            &mut group,
            format!("{label}_png"),
            ctx.value,
            ctx.now,
            OutputForm::Relative,
            OutputFormat::Png,
        );
    }

    let now_ts = now();
    bench_end_to_end_case(
        &mut group,
        "absolute_svg".to_string(),
        now_ts,
        now_ts,
        OutputForm::Absolute,
        OutputFormat::Svg,
    );
    bench_end_to_end_case(
        &mut group,
        "absolute_png".to_string(),
        now_ts,
        now_ts,
        OutputForm::Absolute,
        OutputFormat::Png,
    );

    group.finish();
}

/// Cost of building a `Rasterizer` (fontdb construction from the bundled
/// fonts), relevant for process startup, not per-request latency.
fn bench_startup(c: &mut Criterion) {
    c.bench_function("rasterizer_new", |b| b.iter(Rasterizer::new));
}

fn fast_criterion() -> Criterion {
    // Every duration bucket runs through several groups; a shorter
    // measurement window keeps the full suite fast without losing precision
    // on these microsecond-to-millisecond-scale operations.
    Criterion::default()
        .measurement_time(Duration::from_secs(2))
        .warm_up_time(Duration::from_secs(1))
}

criterion_group! {
    name = benches;
    config = fast_criterion();
    targets = bench_template, bench_svg_parse, bench_rasterize, bench_encode_png, bench_end_to_end, bench_startup
}
criterion_main!(benches);
