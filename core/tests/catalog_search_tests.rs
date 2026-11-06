use rom_core::api_roms::{query_catalog, query_catalog_with_format};

#[test]
fn catalog_pagination_handles_edge_cases_safely() {
    let empty_page = query_catalog(None, None, None, None, None, 99999, 24);
    assert_eq!(empty_page.items.len(), 0);
    assert!(empty_page.total > 0);
    assert!(empty_page.total_pages > 0);
    assert_eq!(empty_page.page, 99999);

    let zero_page = query_catalog(None, None, None, None, None, 0, 0);
    assert_eq!(zero_page.page, 1);
    assert_eq!(zero_page.per_page, 24);
    assert_eq!(zero_page.items.len(), 24);
}

#[test]
fn catalog_filters_by_region_and_format() {
    let usa_res = query_catalog_with_format(None, Some("USA"), None, None, None, Some("CHD"), 1, 20);
    assert!(usa_res.total > 0);
    for item in &usa_res.items {
        assert_eq!(item.region.to_uppercase(), "USA");
        assert_eq!(item.format.to_uppercase(), "CHD");
        assert_eq!(item.source, "Catálogo Oficial (CHD)");
    }

    let japan_res = query_catalog_with_format(None, Some("Japan"), None, None, None, None, 1, 10);
    assert!(japan_res.total > 0);
    for item in &japan_res.items {
        assert_eq!(item.region, "Japan");
    }
}

#[test]
fn catalog_search_matches_words_and_case_insensitively() {
    let res = query_catalog_with_format(Some("grand theft auto"), None, None, None, None, None, 1, 10);
    assert!(res.total > 0);
    for item in &res.items {
        let title_l = item.title.to_lowercase();
        assert!(title_l.contains("grand"));
        assert!(title_l.contains("theft"));
        assert!(title_l.contains("auto"));
    }
}

#[test]
fn catalog_sort_title_and_size() {
    let asc = query_catalog_with_format(Some("Final Fantasy"), None, None, Some("title_asc"), None, None, 1, 10);
    assert!(asc.items.len() >= 2);
    for i in 0..asc.items.len() - 1 {
        assert!(asc.items[i].title.to_lowercase() <= asc.items[i + 1].title.to_lowercase());
    }

    let size_desc = query_catalog_with_format(Some("Final Fantasy"), None, None, Some("size_desc"), None, None, 1, 10);
    assert!(size_desc.items.len() >= 2);
    for i in 0..size_desc.items.len() - 1 {
        assert!(size_desc.items[i].size >= size_desc.items[i + 1].size);
    }
}

