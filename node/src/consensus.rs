use std::collections::HashMap;
use std::process::id;
use rand::prelude::SliceRandom;
use rand::{Rng, thread_rng};

const NUMBER_OF_NODES: usize = 4;
const NUMBER_OF_BYZANTINE_NODES: usize = (NUMBER_OF_NODES - 1) / 3;
const NUMBER_OF_HONEST_NODES: usize = NUMBER_OF_NODES - NUMBER_OF_BYZANTINE_NODES;
const QUORUM: usize = NUMBER_OF_BYZANTINE_NODES * 2 + 1;
const TOTAL_TXS: usize = 1000;

fn main() {
    let mut total_confirmed = 0;

    for _ in 0..TOTAL_TXS {

        println!("New consensus instance");

        let mut votes: HashMap<(usize, usize), Vec<i32>> = HashMap::new();
        let mut nodes: HashMap<usize, usize> = HashMap::new();
        let mut zero_quorum: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut one_quorum: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut decided: HashMap<usize, i32> = HashMap::new();
        let mut excluded_nodes = vec![];

        /// Initial honest votes
        for i in 0..NUMBER_OF_HONEST_NODES {
            let choices = vec![(0, 1), (1, 1)];
            let honest_vote = choices.choose_weighted(&mut rand::thread_rng(), |item| item.1).unwrap().0;
            let mut honest_votes = vec![];
            let new_choices = vec![(honest_vote, 1), (-1, 1)];
            for _ in 0..NUMBER_OF_NODES {
                let new_honest_vote = new_choices.choose_weighted(&mut rand::thread_rng(), |item| item.1).unwrap().0;
                honest_votes.push(new_honest_vote);
            }
            votes.insert((i, 0), honest_votes);
            nodes.insert(i, 0);
            zero_quorum.insert(i, vec![]);
            one_quorum.insert(i, vec![]);
        }

        /// Initial byzantine votes
        for i in 0..NUMBER_OF_BYZANTINE_NODES {
            let mut byzantine_votes = vec![];
            let byzantine_vote = rand::thread_rng().gen_range(-1..2);
            for _ in 0..NUMBER_OF_NODES {
                byzantine_votes.push(byzantine_vote);
            }
            votes.insert((i + NUMBER_OF_HONEST_NODES, 0), byzantine_votes);
            nodes.insert(i + NUMBER_OF_HONEST_NODES, 0);
        }

        /// Assert that all nodes have voted
        for i in 0..NUMBER_OF_NODES {
            assert_eq!(votes.get(&(i, 0)).unwrap().len(), NUMBER_OF_NODES);
        }

        loop {
            for i in 0..NUMBER_OF_NODES {
                let current_round = *nodes.get(&i).unwrap();
                for j in 0..current_round + 1 {
                    let votes = votes.get(&(i, j)).unwrap();
                    println!("Votes of node {} in round {}: {:?}", i, j, votes);
                }
            }

            // Advance round

            for i in 0..NUMBER_OF_HONEST_NODES {
                let current_round = *nodes.get(&i).unwrap();
                let mut zeros = 0;
                let mut ones = 0;
                for j in 0..NUMBER_OF_NODES {
                    match votes.get(&(j, current_round)) {
                        Some(votes) => {
                            if votes[i] == 0 && !excluded_nodes.contains(&j) {
                                zeros += 1;
                            }
                            if votes[i] == 1 && !excluded_nodes.contains(&j) {
                                ones += 1;
                            }
                        }
                        None => {
                            println!("Node {} hasn't voted in round {} yet!", j, current_round);
                        }
                    }
                }
                if zeros + ones >= QUORUM {
                    if !decided.contains_key(&i) {
                        let mut one = one_quorum.get(&i).unwrap().clone();
                        let mut zero = zero_quorum.get(&i).unwrap().clone();
                        if ones >= QUORUM {
                            one.push(current_round);
                            one_quorum.insert(i, one.clone());
                            votes.insert((i, current_round + 1), vec![1; NUMBER_OF_NODES]);
                            //println!("Node {} voted {} in round {}", i, 1, current_round + 1);
                            if current_round != 0 {
                                if one.contains(&(current_round - 1)) {
                                    decided.insert(i, 1);
                                    println!("Node {} decided {} in round {}", i, 1, current_round + 1);
                                }
                            }
                        } else if zeros >= QUORUM {
                            zero.push(current_round);
                            zero_quorum.insert(i, zero.clone());
                            votes.insert((i, current_round + 1), vec![0; NUMBER_OF_NODES]);
                            if current_round != 0 {
                                if zero.contains(&(current_round - 1)) {
                                    decided.insert(i, 0);
                                    println!("Node {} decided {} in round {}", i, 0, current_round + 1);
                                }
                            }
                        } else {
                            //zero.push(current_round);
                            //if zeros >= QUORUM {
                                //zero_quorum.insert(i, zero.clone());
                            //}
                            votes.insert((i, current_round + 1), vec![0; NUMBER_OF_NODES]);
                            println!("Node {} voted {} in round {}", i, 0, current_round + 1);
                            //if current_round != 0 {
                                //if zero.contains(&(current_round - 1)) {
                                    //decided.insert(i, 0);
                                    //println!("Node {} decided {} in round {}", i, 0, current_round + 1);
                                //}
                            //}
                        }
                    }
                    else {
                        /// Vote the decided value
                        let decided_value = *decided.get(&i).unwrap();
                        votes.insert((i, current_round + 1), vec![decided_value; NUMBER_OF_NODES]);
                        println!("Node {} voted {} in round {}", i, decided_value, current_round + 1);
                    }
                    nodes.insert(i, current_round + 1);
                }
                else {
                    /// Receive delayed honest votes
                    for j in 0..NUMBER_OF_HONEST_NODES {
                        match votes.get(&(j, current_round)) {
                            Some(v) => {
                                let mut new_votes = v.clone();
                                for i in 0..new_votes.len() {
                                    if new_votes[i] == -1 {
                                        if new_votes.contains(&1) {
                                            new_votes[i] = 1;
                                        }
                                        else {
                                            new_votes[i] = 0;
                                        }
                                    }
                                }
                                votes.insert((j, current_round), new_votes);
                            }
                            None => {
                                println!("Node {} hasn't voted in round {} yet!", j, current_round);
                            }
                        }
                    }
                }
            }

            /// Try to conclude
            if decided.len() == NUMBER_OF_HONEST_NODES {
                let d = decided.get(&0).unwrap();
                assert!(decided.iter().all(|(a, b)| b == d));
                println!("Consensus achieved!");
                if d == &1 {
                    total_confirmed += 1;
                }
                break;
            }

            /// New byzantine votes
            for i in 0..NUMBER_OF_BYZANTINE_NODES {
                let current_round = *nodes.get(&(NUMBER_OF_HONEST_NODES + i)).unwrap();
                let old_votes = votes.get(&(NUMBER_OF_HONEST_NODES + i, current_round)).unwrap();
                if old_votes.contains(&1) && old_votes.contains(&0) {
                    excluded_nodes.push(NUMBER_OF_HONEST_NODES + i);
                    println!("Byzantine node {} excluded!", NUMBER_OF_HONEST_NODES + i);
                }
                let mut byzantine_votes = vec![];
                let byzantine_vote = rand::thread_rng().gen_range(-1..2);
                for _ in 0..NUMBER_OF_NODES {
                    byzantine_votes.push(byzantine_vote);
                }
                votes.insert((i + NUMBER_OF_HONEST_NODES, current_round + 1), byzantine_votes);
                nodes.insert(i + NUMBER_OF_HONEST_NODES, current_round + 1);
            }
        }
    }
    println!("{} out of {} txs confirmed!", total_confirmed, TOTAL_TXS);
}
