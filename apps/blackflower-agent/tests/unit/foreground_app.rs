use super::Page;

#[test]
fn page_navigation_wraps() {
    assert_eq!(Page::Host.next(), Page::Overview);
    assert_eq!(Page::Overview.previous(), Page::Host);
}
