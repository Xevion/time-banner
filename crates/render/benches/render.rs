use criterion::{Criterion, criterion_group, criterion_main};
use jiff::Timestamp;
use time_banner_render::{OutputForm, OutputFormat, render_time};

fn bench_render(c: &mut Criterion) {
    let now = Timestamp::now();
    let value = Timestamp::from_second(now.as_second() - 3600).unwrap();

    let cases = [
        ("absolute_svg", OutputForm::Absolute, OutputFormat::Svg),
        ("absolute_png", OutputForm::Absolute, OutputFormat::Png),
        ("relative_svg", OutputForm::Relative, OutputFormat::Svg),
        ("relative_png", OutputForm::Relative, OutputFormat::Png),
    ];

    for (label, form, format) in cases {
        c.bench_function(label, |b| {
            b.iter(|| render_time(value, now, form, format.clone()).unwrap())
        });
    }
}

criterion_group!(benches, bench_render);
criterion_main!(benches);
