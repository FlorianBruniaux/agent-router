include!("lib.rs");
#[cfg(test)] mod supplied_feedback {
    use super::*;
    #[test] fn empty_components() { assert_eq!(score(17,&[],&[],&[]),Ok(17)); }
    #[test] fn sum_before_division() { assert_eq!(score(100,&[3,3],&[9,9],&[19,19]),Ok(99)); }
    #[test] fn bounded_work() { assert_eq!(score(0,&[0;65],&[],&[]),Err("work_limit")); }
    #[test] fn preserve_range() { assert_eq!(score(i64::MIN,&[4],&[],&[]),Err("score_out_of_range")); }
}
