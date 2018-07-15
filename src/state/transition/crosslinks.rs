use std::collections::HashMap;
use std::cmp::min;

use super::crystallized_state::CrystallizedState;
use super::crosslink_record::CrosslinkRecord;
use super::partial_crosslink_record::PartialCrosslinkRecord;
use super::config::Config;

// Returns the maximum possible shards for a given validator_count
// and configuration. 
pub fn get_crosslink_shards_count(
    active_validator_count: &usize, 
    config: &Config) 
    -> u16
{
    let system_shard_count: u16 = config.shard_count;
    let notaries_per_crosslink: u16 = config.notaries_per_crosslink;

    assert!(notaries_per_crosslink > 0, "Shards must require > 0 notaries.");

    let notarisable_shard_count = 
        *active_validator_count as u64 / notaries_per_crosslink as u64;

    min(notarisable_shard_count, system_shard_count as u64) as u16
}

pub fn get_crosslink_shards(
    cry_state: &CrystallizedState, 
    config: &Config) 
    -> Vec<u16>
{
    let max_shard_count: u16 = config.shard_count;
    let first_shard: u16 = cry_state.next_shard;
    assert!(first_shard < max_shard_count, "CrystallizedState.next_shard \
    must be less than Config.shard_count.");

    let shard_count = get_crosslink_shards_count(
        &cry_state.num_active_validators(),
        &config);

    let unwrapped_shards: u16 = min(first_shard + shard_count, max_shard_count);
    let wrapped_shards: u16 = (first_shard + shard_count) % max_shard_count;
    let mut crosslink_shards: Vec<u16> = (first_shard..unwrapped_shards).collect();
    crosslink_shards.append(&mut (0_u16..wrapped_shards).collect());
    crosslink_shards
}

pub fn get_crosslink_notaries(
    cry_state: &CrystallizedState,
    shard_id: &u16,
    crosslink_shards: &Vec<u16>)
    -> Vec<usize>
{
    let shard_crosslink_index = crosslink_shards.iter().
        position(|&s| s == *shard_id);

    match shard_crosslink_index {
        None => panic!("shard_id not in crosslink_shards."),
        Some(i) => {
            let crosslink_shards_count = crosslink_shards.len();
            assert!(crosslink_shards_count > 0,
                    "crosslink_shards_count must be > 0");
            let active_validators = cry_state.num_active_validators();
            assert!(active_validators > 0,
                    "active_validators must be > 0");

            let start = active_validators * i / crosslink_shards_count;
            let end = active_validators * (i + 1) / crosslink_shards_count;

            assert!(cry_state.current_shuffling.len() == active_validators,
                    "Crystallized state shuffling does not match active \
                    validator count");

            cry_state.current_shuffling[start..end].to_vec()
        }
    }
}

pub fn process_crosslinks(
    cry_state: &CrystallizedState,
    partial_crosslinks: &Vec<PartialCrosslinkRecord>,
    config: &Config)
    -> (Vec<i64>, Vec<CrosslinkRecord>)
{
    assert!(partial_crosslinks.len() > 0, "No crosslinks present.");
   
    let mut shard_pc_map: 
        HashMap<u16, (&PartialCrosslinkRecord, u64)> = HashMap::new();

    for pc in partial_crosslinks {
        let vote_count = pc.voter_bitfield.num_true_bits();
        let mut competiting_vote_count = 0;
        match shard_pc_map.get(&pc.shard_id) {
            Some(&competitor) => competiting_vote_count = competitor.1,
            None => {}
        }
        // Here we implicitly avoid adding crosslinks with 0 votes
        // to our shard_pc_map.
        if vote_count > competiting_vote_count {
            shard_pc_map.insert(pc.shard_id, (pc, vote_count));
        }
    }

    let mut new_partial_crosslinks: Vec<&PartialCrosslinkRecord> = Vec::new();
    shard_pc_map.iter_mut()
        .for_each(|(_, v)| new_partial_crosslinks.push(v.0));

    let crosslink_shards = get_crosslink_shards(&cry_state, &config);

    let mut deltas = vec![0_i64; cry_state.num_active_validators()];

    let mut new_crosslink_records: Vec<CrosslinkRecord> 
        = Vec::new();

    for shard_id in &crosslink_shards {
        let notaries_indicies = get_crosslink_notaries(
            &cry_state,
            &shard_id,
            &crosslink_shards);
        let new_partial_crosslink = shard_pc_map.get(&shard_id);
        let previous_crosslink_epoch = 
            match cry_state.crosslink_records.get(*shard_id as usize) {
                None => panic!("shard_id not known by \
                               crystallized state."),
                Some(c) => c.epoch
            };
        let current_epoch = cry_state.current_epoch;
        assert!(current_epoch >= previous_crosslink_epoch, "Previous crosslink \
        epoch cannot be > current epoch.");
        let crosslink_distance = cry_state.current_epoch- previous_crosslink_epoch;
        let online_reward: i64 = if crosslink_distance <= 2 { 3 } else { 0 };
        let offline_penalty: i64 = (crosslink_distance as i64).saturating_mul(2);
        for notary in &notaries_indicies {
            let voted = match new_partial_crosslink {
                None => false,
                Some(pc) => pc.0.voter_bitfield.get_bit(&notary)
            };
            match voted {
                true => deltas[*notary] += online_reward,
                false => deltas[*notary] -= offline_penalty
            };
        }
        match new_partial_crosslink {
            None => {
                new_crosslink_records[*shard_id as usize] =
                    cry_state.crosslink_records[*shard_id as usize].clone()
            },
            Some(pc) => {
                let votes = pc.1;
                if ((votes as usize) * 3) >= (notaries_indicies.len() * 2) {
                    new_crosslink_records[*shard_id as usize] = 
                        CrosslinkRecord {
                            epoch: current_epoch,
                            hash: pc.0.shard_block_hash
                        };
                }

            }
        }
    }
    (deltas, new_crosslink_records)
}


#[cfg(test)]
mod tests {
    use super::*;
    use super::super::shuffling::get_shuffling;
    use super::super::super::validator_record::ValidatorRecord;
    use super::super::super::super::utils::types::Sha256Digest;

    #[test]
    fn test_crosslink_shard_count_with_varying_active_vals() {
        let mut config = Config::standard();

        config.shard_count = 10;
        config.notaries_per_crosslink = 10;

        let mut c = get_crosslink_shards_count(
            &100,
            &config);
        assert_eq!(c, 10);
        
        c = get_crosslink_shards_count(
            &101,
            &config);
        assert_eq!(c, 10);
        
        c = get_crosslink_shards_count(
            &99,
            &config);
        assert_eq!(c, 9);
        
        c = get_crosslink_shards_count(
            &0,
            &config);
        assert_eq!(c, 0);
    }
    
    #[test]
    #[should_panic(expected = "must require > 0 notaries.")]
    fn test_crosslink_shard_count_with_zero_notaries_per_crosslink() {
        let mut config = Config::standard();

        config.shard_count = 10;
        config.notaries_per_crosslink = 0;

        let validators: u16 = 10;

        let _ = get_crosslink_shards_count(
            &(validators as usize),
            &config);
    }

    #[test]
    fn test_crosslink_shard_getter_with_5_shards() {
        let mut cry_state = CrystallizedState::zero();
        let mut config = Config::standard();

        config.shard_count = 5;
        config.notaries_per_crosslink = 2;

        (0..10).for_each(
            |_| cry_state.active_validators.push(
                ValidatorRecord::zero_with_thread_rand_pub_key()));

        cry_state.next_shard = 0;
        let c = get_crosslink_shards(
            &cry_state,
            &config);
        assert_eq!(c, [0, 1, 2, 3, 4]);
        
        cry_state.next_shard = 4;
        let c = get_crosslink_shards(
            &cry_state,
            &config);
        assert_eq!(c, [4, 0, 1, 2, 3]);
        
        cry_state.next_shard = 1;
        let c = get_crosslink_shards(
            &cry_state,
            &config);
        assert_eq!(c, [1, 2, 3, 4, 0]);
        
        cry_state.next_shard = 3;
        let c = get_crosslink_shards(
            &cry_state,
            &config);
        assert_eq!(c, [3, 4, 0, 1, 2]);
    }
    
    #[test]
    #[should_panic(expected = "next_shard must be less than Config.shard_count")]
    fn test_crosslink_shard_getter_with_too_large_next_shard() {
        let mut cry_state = CrystallizedState::zero();
        let mut config = Config::standard();

        config.shard_count = 1;
        config.notaries_per_crosslink = 2;

        (0..2).for_each(
            |_| cry_state.active_validators.push(
                ValidatorRecord::zero_with_thread_rand_pub_key()));

        cry_state.next_shard = 6;
        let _ = get_crosslink_shards(
            &cry_state,
            &config);
    }
    
    #[test]
    fn test_crosslink_notaries_allocation() {
        let mut cry_state = CrystallizedState::zero();
        let mut config = Config::standard();
config.shard_count = 5;
        config.notaries_per_crosslink = 2;

        (0..10).for_each(
            |_| cry_state.active_validators.push(
                ValidatorRecord::zero_with_thread_rand_pub_key()));

        cry_state.next_shard = 0;
        let crosslink_shards = get_crosslink_shards(
            &cry_state,
            &config);

        let s = get_shuffling(
            &Sha256Digest::zero(),
            &cry_state.num_active_validators(),
            &config);
        assert_eq!(s, [0, 9, 7, 6, 4, 1, 8, 5, 2, 3]);
        cry_state.current_shuffling = s.clone();

        let mut n = get_crosslink_notaries(
            &cry_state, 
            &0, 
            &crosslink_shards);
        assert_eq!(n, [0, 9]);
        
        n = get_crosslink_notaries(
            &cry_state, 
            &1, 
            &crosslink_shards);
        assert_eq!(n, [7, 6]);
        
        n = get_crosslink_notaries(
            &cry_state, 
            &2, 
            &crosslink_shards);
        assert_eq!(n, [4, 1]);
        
        n = get_crosslink_notaries(
            &cry_state, 
            &3, 
            &crosslink_shards);
        assert_eq!(n, [8, 5]);
        
        n = get_crosslink_notaries(
            &cry_state, 
            &4, 
            &crosslink_shards);
        assert_eq!(n, [2, 3]);
    }
    
    #[test]
    #[should_panic(expected = "shard_id not in crosslink_shards")]
    fn test_crosslink_notaries_allocation_with_invalid_shard() {
        let mut cry_state = CrystallizedState::zero();
        let mut config = Config::standard();

        config.shard_count = 5;
        config.notaries_per_crosslink = 2;

        (0..10).for_each(
            |_| cry_state.active_validators.push(
                ValidatorRecord::zero_with_thread_rand_pub_key()));

        cry_state.next_shard = 0;
        let crosslink_shards = get_crosslink_shards(
            &cry_state,
            &config);

        cry_state.current_shuffling = get_shuffling(
            &Sha256Digest::zero(),
            &cry_state.num_active_validators(),
            &config);
        
        let _ = get_crosslink_notaries(
            &cry_state, 
            &5, 
            &crosslink_shards);
    }
}
