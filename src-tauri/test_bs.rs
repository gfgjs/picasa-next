fn main() {
    let rows = vec![0.0, 300.0, 600.0, 900.0, 1200.0];
    
    for top_y in [0.0, 50.0, 300.0, 350.0, 1500.0, -100.0].iter() {
        let top_y = *top_y;
        let start_idx = match rows.binary_search_by(|r| r.partial_cmp(&top_y).unwrap()) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let mut end_idx = start_idx;
        let bottom_y = top_y + 500.0;
        while end_idx < rows.len() && rows[end_idx] <= bottom_y {
            end_idx += 1;
        }
        println!("top_y: {}, bottom_y: {}, start: {}, end: {}", top_y, bottom_y, start_idx, end_idx);
    }
}
