// A project's GitHub link: the forms people paste, stored one way, matched against a repository's remotes.
use gizai_core::{db::Db, model::ProjectInput, projects, repo_url, seed::ensure_seed};

#[test]
fn every_usual_github_form_becomes_one_link() {
    for form in ["https://github.com/acme-labs/shop-app", "https://github.com/acme-labs/shop-app.git", "http://www.github.com/acme-labs/shop-app/",
                 "git@github.com:acme-labs/shop-app.git", "ssh://git@github.com/acme-labs/shop-app.git", "github.com/acme-labs/shop-app",
                 "acme-labs/shop-app", "  https://github.com/acme-labs/shop-app/tree/main  "] {
        let r = repo_url::normalize(form).unwrap().unwrap_or_else(|| panic!("{form}"));
        assert_eq!((r.url.as_str(), r.provider.as_str(), r.owner.as_deref(), r.name.as_deref()),
                   ("https://github.com/acme-labs/shop-app", "github", Some("acme-labs"), Some("shop-app")), "{form}");
    }
    assert!(repo_url::normalize("").unwrap().is_none());
    assert!(repo_url::normalize("   ").unwrap().is_none());
}

#[test]
fn other_git_hosts_and_paths_are_kept_as_given() {
    let r = repo_url::normalize("git@gitlab.com:team/app.git").unwrap().unwrap();
    assert_eq!((r.url.as_str(), r.provider.as_str()), ("git@gitlab.com:team/app.git", "git"));
    assert_eq!(repo_url::normalize("/srv/git/app.git").unwrap().unwrap().url, "/srv/git/app.git");
    for bad in ["not a url", "https://github.com/only-owner", "ftp://x"] {
        assert!(repo_url::normalize(bad).is_err(), "{bad} accepted");
    }
}

#[test]
fn a_remote_matches_the_link_whatever_its_form() {
    assert!(repo_url::same_repo("git@github.com:Acme-Labs/shop-app.git", "https://github.com/acme-labs/shop-app"));
    assert!(!repo_url::same_repo("git@github.com:acme-labs/shop-app.git", "https://github.com/acme-labs/shop"));
    assert!(repo_url::same_repo("/srv/git/app.git/", "/srv/git/app.git"));
    assert!(repo_url::same_repo("/srv/git/app", "file:///srv/git/app.git"));
}

#[test]
fn a_project_keeps_its_github_link_next_to_its_local_repository() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let input = ProjectInput { name: "Shop".into(), key: "SH".into(), repo_path: Some("/home/x/shop".into()),
        repo_url: Some("git@github.com:acme-labs/shop-app.git".into()), ..Default::default() };
    let id = projects::create(&db, &s.you_id, input.clone()).unwrap();
    assert_eq!(projects::get(&db, &id).unwrap().repo_url.as_deref(), Some("https://github.com/acme-labs/shop-app"));
    projects::update(&db, &s.you_id, &id, ProjectInput { repo_url: None, ..input.clone() }).unwrap();
    assert_eq!(projects::get(&db, &id).unwrap().repo_url, None, "the link can be removed");
    let e = projects::update(&db, &s.you_id, &id, ProjectInput { repo_path: None, ..input }).unwrap_err();
    assert!(e.to_string().contains("local"), "{e}");
}
