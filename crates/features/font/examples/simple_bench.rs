use rldyourterm_font::GlyphCache;

fn main() {
    // Create cache with small limit to test eviction path
    let mut cache = GlyphCache::new_with_max_entries(8, 16, 100);
    
    // Warmup: populate cache with some glyphs
    for ch in 'a'..='z' {
        cache.get(ch);
    }
    
    // Benchmark 1: Cached glyph lookup (fast path)
    let start = std::time::Instant::now();
    let iterations = 1_000_000;
    for _ in 0..iterations {
        cache.get('a');  // Already cached
    }
    let cached_duration = start.elapsed();
    
    // Benchmark 2: Create fresh cache and measure cold lookups
    let mut fresh_cache = GlyphCache::new_with_max_entries(8, 16, 1000);
    let start = std::time::Instant::now();
    for ch in 'a'..='z' {
        for _ in 0..1000 {
            fresh_cache.get(ch);
        }
    }
    let mixed_duration = start.elapsed();
    
    println!("Benchmark Results");
    println!("=================");
    println!("Cached glyph lookup ({} iterations): {:?}", iterations, cached_duration);
    println!("  Average: {:?} per lookup", cached_duration / iterations as u32);
    println!();
    println!("Mixed workload (26 chars x 1000 lookups): {:?}", mixed_duration);
    println!("  Total operations: {}", 26 * 1000);
    println!("  Average: {:?} per lookup", mixed_duration / (26 * 1000) as u32);
}
