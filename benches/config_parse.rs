use criterion::{black_box, criterion_group, criterion_main, Criterion};

use tmux_layout::{config::Config, tmux::TmuxCommandBuilder};

fn criterion_benchmark(c: &mut Criterion) {
    let config_bytes_toml = include_bytes!("../examples/config/.tmux-layout.toml");
    let config_bytes_yml = include_bytes!("../examples/config/.tmux-layout.yml");

    c.bench_function("build_command", |b| {
        let config_str_toml = std::str::from_utf8(config_bytes_toml).unwrap();
        let config = toml::from_str::<Config>(config_str_toml).unwrap();

        b.iter(|| {
            TmuxCommandBuilder::new("tmux", std::iter::empty::<String>())
                .new_sessions(&config.sessions)
                .into_command()
        })
    });
    c.bench_function("parse_config_yml", |b| {
        b.iter(|| {
            serde_yaml::from_slice::<Config>(black_box(config_bytes_yml)).unwrap();
        })
    });
    c.bench_function("parse_config_toml", |b| {
        b.iter(|| {
            let config_str_toml = std::str::from_utf8(config_bytes_toml).unwrap();
            toml::from_str::<Config>(black_box(config_str_toml)).unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
