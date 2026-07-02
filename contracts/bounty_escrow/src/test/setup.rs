use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

use crate::{BountyEscrow, BountyEscrowClient};

pub const UNIT: i128 = 10_000_000;
pub const DAY: u64 = 86_400;
pub const GRACE: u64 = 3 * DAY;
pub const FEE_BPS: u32 = 300;
pub const REPO: u64 = 42;

pub struct Ctx<'a> {
    pub env: Env,
    pub client: BountyEscrowClient<'a>,
    pub admin: Address,
    pub relayer: Address,
    pub arbiter: Address,
    pub treasury: Address,
    pub maintainer: Address,
    pub winner: Address,
    pub token: Address,
    pub token2: Address,
}

impl<'a> Ctx<'a> {
    pub fn new() -> Ctx<'a> {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let id = env.register(BountyEscrow, ());
        let client = BountyEscrowClient::new(&env, &id);
        let admin = Address::generate(&env);
        let relayer = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let treasury = Address::generate(&env);
        let maintainer = Address::generate(&env);
        let winner = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        let token2 = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        client.init(
            &admin,
            &relayer,
            &arbiter,
            &treasury,
            &FEE_BPS,
            &GRACE,
            &DAY,
            &(90 * DAY),
        );
        for t in [&token, &token2] {
            client.set_token(&admin, t, &UNIT, &(500 * UNIT));
            StellarAssetClient::new(&env, t).mint(&maintainer, &(10_000 * UNIT));
        }
        Ctx {
            env,
            client,
            admin,
            relayer,
            arbiter,
            treasury,
            maintainer,
            winner,
            token,
            token2,
        }
    }

    pub fn now(&self) -> u64 {
        self.env.ledger().timestamp()
    }

    pub fn set_time(&self, t: u64) {
        self.env.ledger().set_timestamp(t);
    }

    pub fn bal(&self, token: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, token).balance(who)
    }

    pub fn contract_bal(&self, token: &Address) -> i128 {
        self.bal(token, &self.client.address)
    }

    /// Fund 100 units on `issue`, 7 days, relayer allowed.
    pub fn fund(&self, issue: u64) -> u64 {
        self.fund_with(issue, 100 * UNIT, true)
    }

    pub fn fund_with(&self, issue: u64, amount: i128, relayer: bool) -> u64 {
        self.client.fund_bounty(
            &self.maintainer,
            &REPO,
            &issue,
            &self.token,
            &amount,
            &(self.now() + 7 * DAY),
            &relayer,
        )
    }

    pub fn assert_invariant(&self, token: &Address) {
        let t = self.client.get_totals(token);
        assert_eq!(t.funded, t.released_net + t.fees + t.refunded + t.held);
        assert_eq!(t.held, self.contract_bal(token));
    }
}
