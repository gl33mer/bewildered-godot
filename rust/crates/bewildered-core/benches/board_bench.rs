use bewildered_core::{Board, GemKind};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_match_scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("match_scan");

    for size in [8, 12, 16] {
        group.bench_with_input(BenchmarkId::new("board_size", size), &size, |b, &size| {
            let board = Board::new(
                size,
                size,
                42,
                vec![
                    GemKind::Circle,
                    GemKind::Triangle,
                    GemKind::Square,
                    GemKind::Diamond,
                ],
            );
            b.iter(|| {
                let matches = board.find_all_matches();
                black_box(matches);
            });
        });
    }
    group.finish();
}

fn bench_cascade_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("cascade_resolution");

    for size in [8, 12, 16] {
        group.bench_with_input(BenchmarkId::new("board_size", size), &size, |b, &size| {
            let board = Board::new(
                size,
                size,
                42,
                vec![
                    GemKind::Circle,
                    GemKind::Triangle,
                    GemKind::Square,
                    GemKind::Diamond,
                ],
            );
            b.iter(|| {
                let mut board = board.clone();
                // Create a guaranteed match at top-left
                board.set_gem(0, 0, GemKind::Circle);
                board.set_gem(0, 1, GemKind::Circle);
                board.set_gem(0, 2, GemKind::Circle);
                let matches = board.find_all_matches();
                if !matches.is_empty() {
                    let _ = board.try_swap(0, 0, 0, 1);
                }
                black_box(board);
            });
        });
    }
    group.finish();
}

fn bench_legal_move_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("legal_move_detection");

    for size in [8, 12, 16] {
        group.bench_with_input(BenchmarkId::new("board_size", size), &size, |b, &size| {
            let board = Board::new(
                size,
                size,
                42,
                vec![
                    GemKind::Circle,
                    GemKind::Triangle,
                    GemKind::Square,
                    GemKind::Diamond,
                ],
            );
            b.iter(|| {
                let has_moves = board.has_legal_moves();
                black_box(has_moves);
            });
        });
    }
    group.finish();
}

fn bench_full_move(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_move");

    for size in [8, 12, 16] {
        group.bench_with_input(BenchmarkId::new("board_size", size), &size, |b, &size| {
            let board = Board::new(
                size,
                size,
                42,
                vec![
                    GemKind::Circle,
                    GemKind::Triangle,
                    GemKind::Square,
                    GemKind::Diamond,
                ],
            );
            b.iter(|| {
                let mut board = board.clone();
                // Try a swap that creates a match
                board.set_gem(0, 0, GemKind::Circle);
                board.set_gem(0, 1, GemKind::Triangle);
                board.set_gem(0, 2, GemKind::Circle);
                board.set_gem(0, 3, GemKind::Circle);
                let _ = board.try_swap(0, 1, 0, 2);
                black_box(board);
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_match_scan,
    bench_cascade_resolution,
    bench_legal_move_detection,
    bench_full_move
);
criterion_main!(benches);
