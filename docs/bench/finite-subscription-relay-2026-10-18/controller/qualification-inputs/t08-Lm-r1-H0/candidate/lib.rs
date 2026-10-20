pub fn metrics(costs:&[Option<u64>],outcomes:&[Option<bool>])->Result<Option<(u128,u64)>, &'static str> {
 if costs.len()!=outcomes.len() {return Err("length");}
 if costs.len()>1000 {return Err("limit");}
 if costs.iter().any(Option::is_none)||outcomes.iter().any(Option::is_none) {return Ok(None);}
 let accepted=outcomes.iter().filter(|x|**x==Some(true)).count() as u64;
 if accepted==0 {return Ok(None);}
 let total=costs.iter().map(|x|u128::from(x.unwrap())).sum();
 Ok(Some((total,accepted)))
}
