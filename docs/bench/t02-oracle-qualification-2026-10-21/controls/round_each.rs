pub fn score(
    quality: i64,
    costs: &[u64],
    latencies: &[u64],
    skills: &[u64],
) -> Result<i64, &'static str> {
    if costs.len() > 64 || latencies.len() > 64 || skills.len() > 64 {
        return Err("work_limit");
    }
    let component = |values: &[u64], divisor: u64| {
        values
            .iter()
            .map(|v| i128::from(*v / divisor))
            .sum::<i128>()
    };
    let value = i128::from(quality) - component(costs, 4) - component(latencies, 10)
        + component(skills, 20);
    i64::try_from(value).map_err(|_| "score_out_of_range")
}
