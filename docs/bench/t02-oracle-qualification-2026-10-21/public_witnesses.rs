#[cfg(test)]
mod qualification_witnesses {
    use super::*;
    #[test]
    fn qa174_sum_cost_above_u64() {
        assert_eq!(
            score(0, &[u64::MAX, 1], &[], &[]),
            Ok(-4_611_686_018_427_387_904)
        );
    }
    #[test]
    fn qa174_sum_latency_above_u64() {
        assert_eq!(
            score(0, &[], &[u64::MAX, 1], &[]),
            Ok(-1_844_674_407_370_955_161)
        );
    }
    #[test]
    fn qa174_sum_skills_above_u64() {
        assert_eq!(
            score(0, &[], &[], &[u64::MAX, 1]),
            Ok(922_337_203_685_477_580)
        );
    }
    #[test]
    fn qa174_round_cost_after_sum() {
        assert_eq!(score(0, &[3, 1], &[], &[]), Ok(-1));
    }
    #[test]
    fn qa174_round_latency_after_sum() {
        assert_eq!(score(0, &[], &[9, 1], &[]), Ok(-1));
    }
    #[test]
    fn qa174_round_skills_after_sum() {
        assert_eq!(score(0, &[], &[], &[19, 1]), Ok(1));
    }
    #[test]
    fn qa174_cost_work_bound() {
        assert_eq!(score(0, &[0; 65], &[], &[]), Err("work_limit"));
    }
    #[test]
    fn qa174_latency_work_bound() {
        assert_eq!(score(0, &[], &[0; 65], &[]), Err("work_limit"));
    }
    #[test]
    fn qa174_skills_work_bound() {
        assert_eq!(score(0, &[], &[], &[0; 65]), Err("work_limit"));
    }
    #[test]
    fn qa174_inclusive_work_bound() {
        assert_eq!(score(0, &[0; 64], &[0; 64], &[0; 64]), Ok(0));
    }
    #[test]
    fn qa174_final_lower_bound() {
        assert_eq!(score(i64::MIN, &[4], &[], &[]), Err("score_out_of_range"));
    }
    #[test]
    fn qa174_final_upper_bound() {
        assert_eq!(score(i64::MAX, &[], &[], &[20]), Err("score_out_of_range"));
    }
    #[test]
    fn qa174_empty_extremes() {
        assert_eq!(score(i64::MIN, &[], &[], &[]), Ok(i64::MIN));
        assert_eq!(score(i64::MAX, &[], &[], &[]), Ok(i64::MAX));
    }
}
