pub fn score(quality: i64, costs: &[u64], latencies: &[u64], skills: &[u64]) -> Result<i64, &'static str> {
    let cost: u64 = costs.iter().map(|x| x / 4).sum();
    let latency: u64 = latencies.iter().map(|x| x / 10).sum();
    let skill: u64 = skills.iter().map(|x| x / 20).sum();
    Ok(quality - cost as i64 - latency as i64 + skill as i64)
}
