use crate::{
    storage::MAX_BOUNTIES_LIMIT,
    test::setup::{Ctx, UNIT},
};

#[test]
fn paginated_get_bounties_works() {
    let c = Ctx::new();

    // Create 5 bounties
    let mut created_ids = soroban_sdk::Vec::new(&c.env);
    for i in 1..=5 {
        let id = c.fund_with(100 + i, (10 + i as i128) * UNIT, false);
        created_ids.push_back(id);
    }

    // Page from start = 1, limit = 2
    let page1 = c.client.get_bounties(&1, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().id, 1);
    assert_eq!(page1.get(1).unwrap().id, 2);

    // Page from start = 3, limit = 2
    let page2 = c.client.get_bounties(&3, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap().id, 3);
    assert_eq!(page2.get(1).unwrap().id, 4);

    // Page from start = 5, limit = 2 (only 1 remaining)
    let page3 = c.client.get_bounties(&5, &2);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().id, 5);

    // Paging across the end returns empty vector
    let end_page = c.client.get_bounties(&6, &10);
    assert_eq!(end_page.len(), 0);

    let far_page = c.client.get_bounties(&1000, &10);
    assert_eq!(far_page.len(), 0);

    // Hard cap enforced
    let capped_page = c.client.get_bounties(&1, &100);
    assert!(capped_page.len() <= MAX_BOUNTIES_LIMIT);
}

#[test]
fn paginated_get_bounties_skips_missing_ids() {
    let c = Ctx::new();

    // Start = 0 (id 0 was never created)
    let id1 = c.fund(1);
    let id2 = c.fund(2);

    let list = c.client.get_bounties(&0, &5);
    assert_eq!(list.len(), 2);
    assert_eq!(list.get(0).unwrap().id, id1);
    assert_eq!(list.get(1).unwrap().id, id2);
}
