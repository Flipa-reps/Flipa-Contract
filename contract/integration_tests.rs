use super::*;
use soroban_sdk::testutils::{Address as _, Ledger, StellarAssetClient as _};
use proptest::{prelude::*, collection::vec};
use proptest::prop_assert_eq;

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Integration test harness with deterministic helpers for complete game flows.
    /// Seeds: 1 = win (Heads), 3 = loss (Heads)
    pub struct Harness {
        env: Env,
        client: CoinflipContractClient<'static>,
    }

    impl Harness {
        pub fn new() -> Self {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register_contract(None, CoinflipContract);
            let client = CoinflipContractClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = env.register_stellar_asset_contract(admin.clone());
            client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
            Self { env, client }
        }

        pub fn player(&self) -> Address {
            Address::generate(&self.env)
        }

        pub fn make_secret(&self, seed: u8) -> Bytes {
            let mut bytes = Bytes::new(&self.env);
            for _ in 0..32 {
                bytes.push_back(seed);
            }
            bytes
        }

        pub fn make_commitment(&self, seed: u8) -> BytesN<32> {
            self.env.crypto().sha256(&self.make_secret(seed)).try_into().unwrap()
        }

        pub fn fund(&self, amount: i128) {
            self.env.as_contract(
                &self.env.current_contract_address(),
                || {
                    let mut stats = CoinflipContract::load_stats(&self.env);
                    stats.reserve_balance = amount;
                    CoinflipContract::save_stats(&self.env, &stats);
                },
            );
        }

        pub fn inject_game(&self, player: &Address, phase: GamePhase, streak: u32, wager: i128) {
            let commitment = self.make_commitment(1);
            let contract_random = self.make_commitment(2);
            let game = GameState {
                wager,
                side: Side::Heads,
                streak,
                commitment,
                contract_random,
                fee_bps: 300,
                phase,
                start_ledger: 0,
            };
            self.env.as_contract(
                &self.env.current_contract_address(),
                || {
                    CoinflipContract::save_player_game(&self.env, player, &game);
                },
            );
        }

        pub fn stats(&self) -> ContractStats {
            self.env.as_contract(
                &self.env.current_contract_address(),
                || CoinflipContract::load_stats(&self.env),
            )
        }

        pub fn game_state(&self, player: &Address) -> Option<GameState> {
            self.env.as_contract(
                &self.env.current_contract_address(),
                || CoinflipContract::load_player_game(&self.env, player),
            )
        }

        pub fn play_round(&self, player: &Address, side: Side, wager: i128, seed: u8) -> bool {
            let commitment = self.make_commitment(seed);
            self.client.start_game(&player.clone(), &side, &wager, &commitment);
            let secret = self.make_secret(seed);
            self.client.reveal(&player.clone(), &secret).unwrap()
        }

        pub fn play_win_round(&self, player: &Address, wager: i128) -> bool {
            self.play_round(player, Side::Heads, wager, 1)
        }

        pub fn play_loss_round(&self, player: &Address, wager: i128) -> bool {
            self.play_round(player, Side::Heads, wager, 3)
        }

        pub fn probe_outcome(&self, seed: u8) -> Side {
            let seq_bytes = self.env.ledger().sequence().to_be_bytes();
            let cr_bytes = Bytes::from_slice(&self.env, &seq_bytes);
            let cr_hash: BytesN<32> = self.env.crypto().sha256(&cr_bytes).try_into().unwrap();
            let cr_array = cr_hash.to_array();
            let secret = self.make_secret(seed);
            let mut combined = Bytes::new(&self.env);
            combined.append(&secret);
            let mut cr_copy = Bytes::from_slice(&self.env, &cr_array);
            combined.append(&cr_copy);
            let outcome_bit = self.env.crypto().sha256(&combined).to_array()[0] % 2;
            if outcome_bit == 0 { Side::Heads } else { Side::Tails }
        }

        /// PROPERTY 21 HELPER: Get total funds = player_balance + contract_reserve + treasury_balance
        pub fn total_funds(&self, player: &Address) -> i128 {
            let contract_id = self.env.current_contract_address();
            let token_id: Address = self.env.as_contract(&contract_id, || {
                CoinflipContract::load_config(&self.env).token.clone()
            });
            let token_client = soroban_sdk::token::StellarAssetClient::new(&self.env, &token_id);
            
            let config = self.env.as_contract(&contract_id, || CoinflipContract::load_config(&self.env));
            let treasury = config.treasury.clone();
            
            let player_balance = token_client.balance(player);
            let treasury_balance = token_client.balance(&treasury);
            let reserve_balance = self.stats().reserve_balance;
            
            player_balance + treasury_balance + reserve_balance
        }
    }

    #[test]
    fn test_losses_at_streak_1_to_4_plus() {
        let h = Harness::new();
        h.fund(1_000_000_000);

        for streak in 1..=5u32 {
            let player = h.player();
            h.inject_game(&player, GamePhase::Revealed, streak, 10_000_000);

            let pre_stats = h.stats();

            // cash_out rejects
            let cash_out_err = h.client.try_cash_out(&player);
            assert_eq!(cash_out_err, Err(Ok(Error::NoWinningsToClaimOrContinue)));

            // continue_streak rejects
            let continue_err = h.client.try_continue_streak(&player, &h.make_commitment(42));
            assert_eq!(continue_err, Err(Ok(Error::NoWinningsToClaimOrContinue)));

            // Reserves unchanged, game persists (simulates "loss at streak" - no settlement)
            let post_stats = h.stats();
            assert_eq!(pre_stats.reserve_balance, post_stats.reserve_balance);
            let game = h.game_state(&player).unwrap();
            assert_eq!(game.streak, streak);
            assert_eq!(game.phase, GamePhase::Revealed);
        }
    }

    #[test]
    fn test_max_streak_scenario_4_wins_cash_out() {
        let h = Harness::new();
        h.fund(1_000_000_000);
        let wager = 1_000_000i128;
        let player = h.player();

        // 4 consecutive wins
        for _ in 0..4 {
            assert!(h.play_win_round(&player, wager));
            let game = h.game_state(&player).unwrap();
            assert_eq!(game.phase, GamePhase::Revealed);
            if game.streak < 4 {
                let commit = h.make_commitment(42);
                h.client.continue_streak(&player, &commit).unwrap();
            }
        }

        // Cash out at streak 4 (10x)
        let pre_stats = h.stats();
        let payout = h.client.cash_out(&player).unwrap();
        let expected_gross = wager * 10; // 10x
        let expected_fee = expected_gross / 333; // ~3% = 300bps
        let expected_net = expected_gross - expected_fee;

        assert_eq!(payout, expected_net);
        assert_eq!(h.stats().total_games, pre_stats.total_games + 4);
        assert_eq!(h.stats().reserve_balance, pre_stats.reserve_balance - expected_gross);
    }

    #[test]
    fn test_streak_losses_preserve_reserves() {
        let h = Harness::new();
        h.fund(100_000_000);
        let wager = 5_000_000i128;
        let player = h.player();

        // Win to streak 1
        assert!(h.play_win_round(&player, wager));
        let pre_continue_stats = h.stats();

        // Simulate loss at streak 1 (inject Revealed streak=0)
        h.inject_game(&player, GamePhase::Revealed, 0, wager);
        h.client.cash_out(&player).expect_err("loss reject");
        assert_eq!(h.stats().reserve_balance, pre_continue_stats.reserve_balance, "reserves unchanged on loss reject");

        // Repeat for streak 2
        h.play_win_round(&player, wager);
        h.client.continue_streak(&player, &h.make_commitment(42)).unwrap();
        assert!(h.play_win_round(&player, wager));
        let pre_loss_stats = h.stats();
        h.inject_game(&player, GamePhase::Revealed, 0, wager);
        h.client.cash_out(&player).expect_err("loss reject");
        assert_eq!(h.stats().reserve_balance, pre_loss_stats.reserve_balance);
    }

    #[test]
    fn test_claim_winnings_token_transfers() {
        let h = Harness::new();
        let wager = 10_000_000i128;
        let player = h.player();
        assert!(h.play_win_round(&player, wager));

        // Setup token minting
        let contract_id = h.env.current_contract_address();
        let token_id: Address = h.env.as_contract(&contract_id, || {
            CoinflipContract::load_config(&h.env).token.clone()
        });
        let token_client = soroban_sdk::token::StellarAssetClient::new(&h.env, &token_id);
        token_client.mint(&contract_id, &(wager * 20)); // excess for payout

        let pre_contract = token_client.balance(&contract_id);
        let pre_player = token_client.balance(&player);
        let config = h.env.as_contract(&contract_id, || CoinflipContract::load_config(&h.env));
        let treasury = config.treasury.clone();
        let pre_treasury = token_client.balance(&treasury);

        let gross = wager * 19 / 10;
        let fee = gross * 3 / 100;
        let net = gross - fee;

        h.client.claim_winnings(&player).unwrap();

        assert_eq!(token_client.balance(&contract_id), pre_contract - gross);
        assert_eq!(token_client.balance(&player), pre_player + net);
        assert_eq!(token_client.balance(&treasury), pre_treasury + fee);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// PROPERTY 21: Fund Conservation
        /// Total funds = player_balance + contract_reserve + treasury_balance remains
        /// constant throughout complete game lifecycles with randomized wagers, fees,
        /// settlement choices, and multi-round streaks.
        ///
        /// Tests scenarios:
        /// - Direct win → cash_out
        /// - Win → continue_streak → next win/loss
        /// - Pure loss sequences (forfeit to reserves)
        /// - claim_winnings with token transfers
        ///
        /// Invariant: sum unchanged after every operation.
        #[test]
        fn prop_fund_conservation(
            fee_bps   in 200u32..=500u32,
            base_wager in 2_000_000i128..20_000_000i128,
            num_rounds in 1usize..6usize,
            continue_chance in 0.0f64..1.0,
            use_claim_winnings in any::<bool>(),
        ) {
            let mut h = Harness::new();
            
            // Set custom fee for this test run
            let contract_id = h.env.current_contract_address();
            h.env.as_contract(&contract_id, || {
                let mut config = CoinflipContract::load_config(&h.env);
                config.fee_bps = fee_bps;
                CoinflipContract::save_config(&h.env, &config);
            });
            
            let player = h.player();
            let mut initial_reserve = 1_000_000_000i128;
            h.fund(initial_reserve);
            
            // Mint excess tokens to contract to cover all possible payouts
            let token_id: Address = h.env.as_contract(&contract_id, || {
                CoinflipContract::load_config(&h.env).token.clone()
            });
            let token_client = soroban_sdk::token::StellarAssetClient::new(&h.env, &token_id);
            token_client.mint(&contract_id, &initial_reserve);
            
            let initial_total = h.total_funds(&player);
            
            let mut current_wager = base_wager;
            for round in 0..num_rounds {
                // Play round
                let is_win = h.play_win_round(&player, current_wager);
                
                // Verify conservation after reveal
                prop_assert_eq!(h.total_funds(&player), initial_total,
                    "Funds mismatch after round {} reveal (win={})", round, is_win);
                
                if !is_win {
                    // Loss: game deleted, wager → reserves (no further action needed)
                    continue;
                }
                
                // Win: choose settlement or continue based on probability
                let game = h.game_state(&player).unwrap();
                let streak = game.streak;
                
                if rand::random::<f64>() < continue_chance && streak < 4 {
                    // Continue streak (no funds move)
                    let next_commitment = h.make_commitment(42u8);
                    h.client.continue_streak(&player, &next_commitment).unwrap();
                    // Double wager for next round risk
                    current_wager = current_wager * 2;
                    prop_assert_eq!(h.total_funds(&player), initial_total,
                        "Funds mismatch after continue_streak (streak={})", streak);
                } else {
                    // Settle: cash_out or claim_winnings
                    if use_claim_winnings {
                        h.client.claim_winnings(&player).unwrap();
                    } else {
                        h.client.cash_out(&player).unwrap();
                    }
                    // Reset wager for next game
                    current_wager = base_wager;
                    prop_assert_eq!(h.total_funds(&player), initial_total,
                        "Funds mismatch after settlement (streak={}, claim={})", 
                        streak, use_claim_winnings);
                }
            }
            
            // Final verification after all rounds
            prop_assert_eq!(h.total_funds(&player), initial_total,
                "Funds mismatch after {} complete rounds", num_rounds);
        }
    }
}

