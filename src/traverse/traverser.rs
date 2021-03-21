use strip_markdown::strip_markdown;
use toml::Value;
use walkdir::{DirEntry, WalkDir};
use yaml_rust::YamlLoader;

use num_cpus;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use threadpool::ThreadPool;

use crate::constants;
use crate::file_location::*;
use crate::hugo_to_json_error::*;
use crate::operation_result::*;
use crate::page_index::PageIndex;

pub struct Traverser {
    contents_directory_path: PathBuf,
    drafts: bool, // Include drafts.
}

impl Traverser {
    pub fn new(contents_directory_path: PathBuf, drafts: bool) -> Self {
        Self {
            contents_directory_path,
            drafts,
        }
    }

    /// Uses multiple threads to traverse
    pub fn traverse_files(
        &self,
    ) -> Result<Vec<Result<PageIndex, OperationResult>>, HugotoJsonError> {
        let mut index = Vec::new();
        let drafts = self.drafts;

        // TODO: Attempt to use Raynon for a speed increase.

        let thread_count = num_cpus::get();
        let pool = ThreadPool::new(thread_count);
        let (tx, rx) = channel();

        // This errors early if the path doesn't exist
        fs::metadata(&self.contents_directory_path)?;

        for entry in WalkDir::new(&self.contents_directory_path)
            .into_iter()
            .filter_entry(|e| !is_hidden(e))
        {
            match entry {
                Ok(ref file) => {
                    let file_location = FileLocation::new(file, &self.contents_directory_path);
                    // TODO: What should be done in this case?
                    if file_location.is_err() {
                        continue;
                    }

                    let thread_tx = tx.clone();
                    let file_location = file_location.unwrap();

                    pool.execute(move || {
                        debug!("Processing {}", &file_location);
                        let process_result = process_file(&file_location, drafts);
                        thread_tx.send(process_result).expect("Channel exists");
                    });
                }
                Err(error) => {
                    if let Some(io_error) = error.into_io_error() {
                        error!("Failed {}", io_error)
                    } else {
                        error!("Error reading unknown file")
                    }
                }
            }
        }

        // This sender must be dropped as otherwise the iterator blocks as it's possible for the channel to still send messages
        drop(tx);

        for result in rx {
            match result {
                Err(OperationResult::Skip(ref err)) => warn!("{}", err), // Skips don't need to be handled
                Err(OperationResult::Path(ref err)) => {
                    error!("{}", err);
                    index.push(result);
                }
                Err(OperationResult::Parse(ref err)) => {
                    error!("{}", err);
                    index.push(result);
                }
                Err(OperationResult::Io(ref err)) => {
                    error!("{}", err);
                    index.push(result);
                }
                Ok(_) => index.push(result),
            }
        }

        pool.join();
        Ok(index)
    }
}

fn process_file(file_location: &FileLocation, drafts: bool) -> Result<PageIndex, OperationResult> {
    match file_location.extension.as_ref() {
        constants::MARKDOWN_EXTENSION => process_md_file(&file_location, drafts),
        // TODO: .html files
        _ => Err(OperationResult::Path(PathError::new(
            &file_location.absolute_path,
            "Not a compatible file extension.",
        ))),
        // TODO: Handle None
    }
}

fn process_md_file(
    file_location: &FileLocation,
    drafts: bool,
) -> Result<PageIndex, OperationResult> {
    let contents = fs::read_to_string(file_location.absolute_path.to_string())?;
    let first_line = contents.lines().find(|&l| !l.trim().is_empty());

    match first_line.unwrap_or_default().chars().next() {
        Some('+') => process_md_toml_front_matter(&contents, &file_location, drafts),
        Some('-') => process_md_yaml_front_matter(&contents, &file_location, drafts),
        // TODO: JSON frontmatter '{' => process_json_frontmatter()
        _ => Err(OperationResult::Parse(ParseError::new(
            &file_location.absolute_path,
            "Could not determine file front matter type.",
        ))),
    }
}

fn process_md_toml_front_matter(
    contents: &str,
    file_location: &FileLocation,
    drafts: bool,
) -> Result<PageIndex, OperationResult> {
    let split_content: Vec<&str> = contents.trim().split(constants::TOML_FENCE).collect();

    let length = split_content.len();
    if length <= 1 {
        return Err(OperationResult::Parse(ParseError::new(
            &file_location.absolute_path,
            "Could not split on TOML fence.",
        )));
    }

    let front_matter = split_content[length - 2]
        .trim()
        .parse::<Value>()
        .map_err(|_| {
            ParseError::new(
                &file_location.absolute_path,
                "Could not parse TOML front matter.",
            )
        })?;
    let is_draft = front_matter
        .get(constants::DRAFT)
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if is_draft && !drafts {
        return Err(OperationResult::Skip(Skip::new(
            &file_location.absolute_path,
            "Is draft.",
        )));
    }

    let title = front_matter.get(constants::TITLE).and_then(Value::as_str);
    let slug = front_matter.get(constants::SLUG).and_then(Value::as_str);
    let date = front_matter.get(constants::DATE).and_then(Value::as_str);
    let description = front_matter
        .get(constants::DESCRIPTION)
        .and_then(Value::as_str);
    let url = front_matter.get(constants::URL).and_then(Value::as_str);

    let categories: Vec<String> = front_matter
        .get(constants::CATEGORIES)
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_owned()))
        .collect();

    let series: Vec<String> = front_matter
        .get(constants::SERIES)
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_owned()))
        .collect();

    let tags: Vec<String> = front_matter
        .get(constants::TAGS)
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_owned()))
        .collect();

    let keywords: Vec<String> = front_matter
        .get(constants::KEYWORDS)
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_owned()))
        .collect();

    let content = strip_markdown(split_content[length - 1].trim());

    PageIndex::new(
        title,
        slug,
        date,
        description,
        categories,
        series,
        tags,
        keywords,
        content,
        &file_location,
        url,
        is_draft,
    )
}

fn process_md_yaml_front_matter(
    contents: &str,
    file_location: &FileLocation,
    drafts: bool,
) -> Result<PageIndex, OperationResult> {
    let split_content: Vec<&str> = contents.trim().split(constants::YAML_FENCE).collect();
    let length = split_content.len();
    if length <= 1 {
        return Err(OperationResult::Parse(ParseError::new(
            &file_location.absolute_path,
            "Could not split on YAML fence.",
        )));
    }

    let front_matter = split_content[1].trim();
    let front_matter = YamlLoader::load_from_str(front_matter).map_err(|_| {
        ParseError::new(
            &file_location.absolute_path,
            "Could not parse YAML front matter.",
        )
    })?;
    let front_matter = front_matter.first().ok_or_else(|| {
        ParseError::new(
            &file_location.absolute_path,
            "Could not parse YAML front matter.",
        )
    })?;

    let is_draft = front_matter[constants::DRAFT].as_bool().unwrap_or(false);

    if is_draft && !drafts {
        return Err(OperationResult::Skip(Skip::new(
            &file_location.absolute_path,
            "Is draft.",
        )));
    }

    let title = front_matter[constants::TITLE].as_str();
    let slug = front_matter[constants::SLUG].as_str();
    let description = front_matter[constants::DESCRIPTION].as_str();
    let date = front_matter[constants::DATE].as_str();
    let url = front_matter[constants::URL].as_str();

    let series: Vec<String> = front_matter[constants::SERIES]
        .as_vec()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_owned()))
        .collect();

    let categories: Vec<String> = front_matter[constants::CATEGORIES]
        .as_vec()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_owned()))
        .collect();

    let tags: Vec<String> = front_matter[constants::TAGS]
        .as_vec()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_owned()))
        .collect();

    let keywords: Vec<String> = front_matter[constants::KEYWORDS]
        .as_vec()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_owned()))
        .collect();

    let content = strip_markdown(split_content[length - 1].trim());

    PageIndex::new(
        title,
        slug,
        date,
        description,
        categories,
        series,
        tags,
        keywords,
        content,
        &file_location,
        url,
        is_draft,
    )
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map_or(false, |s| s.starts_with('.'))
}

#[cfg(test)]
mod test {
    use super::*;

    fn build_file_location() -> FileLocation {
        FileLocation {
            extension: String::from("md"),
            relative_directory_to_content: String::from("post"),
            absolute_path: String::from("/home/blog/content/post/example.md"),
            file_name: String::from("example.md"),
            file_stem: String::from("example"),
        }
    }

    #[test]
    fn page_index_from_yaml() {
        let contents = String::from(
            r#"
---
draft: false
title: Responsive Blog Images
date: "2019-01-20T23:11:28Z"
slug: responsive-blog-images
tags:
  - Hugo
  - Images
  - Responsive
  - Blog
---
The state of images on the web is pretty rough. What should be an easy goal, showing a user a picture, is...
"#,
        );
        let page_index = process_md_yaml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_ok());
        let page_index = page_index.unwrap();
        assert_eq!(page_index.title, "Responsive Blog Images");
        assert_eq!(page_index.content, "The state of images on the web is pretty rough. What should be an easy goal, showing a user a picture, is...");
        assert_eq!(page_index.date, "2019-01-20T23:11:28Z");
        assert_eq!(
            page_index.tags,
            vec!["Hugo", "Images", "Responsive", "Blog"]
        );

        // Should be empty as not provided
        assert!(page_index.series.is_empty());
        assert!(page_index.keywords.is_empty());
        assert!(page_index.description.is_empty());
        assert!(page_index.categories.is_empty());
    }

    #[test]
    fn page_index_from_yaml_returns_skip_err_when_draft() {
        let contents = String::from(
            r#"
---
draft: true
title: Responsive Blog Images
date: "2019-01-20T23:11:28Z"
slug: responsive-blog-images
tags:
  - Hugo
  - Images
  - Responsive
  - Blog
---
The state of images on the web is pretty rough. What should be an easy goal, showing a user a picture, is...
"#,
        );
        let page_index = process_md_yaml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_err());
        // Pattern match the error type
        match page_index.unwrap_err() {
            OperationResult::Skip(_) => (), // The case where the result is a Skip result succeeds
            _ => panic!("This should fail"), // All other cases fail
        }
    }

    #[test]
    fn page_index_from_yaml_returns_ok_if_fence_not_closed() {
        let contents = String::from(
            r#"
---
draft: false
title: Responsive Blog Images
date: "2019-01-20T23:11:28Z"
slug: responsive-blog-images
tags:
  - Hugo
  - Images
"#,
        );
        let page_index = process_md_yaml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_ok());
    }

    #[test]
    fn page_index_from_yaml_returns_parse_err_on_malformed_yaml() {
        let contents = String::from(
            r#"
---
title: Responsive Blog Images
date: "2019-01-20T23:11:28Z"
slug: responsive-blog-images
tags
  - :Hugo
  - Images
---
"#,
        );

        let page_index = process_md_yaml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_err());
        // Pattern match error
        match page_index.unwrap_err() {
            OperationResult::Parse(_) => (), // The case where the result is a Parse result succeeds
            _ => panic!("This should not fail"), // All other cases fail
        }
    }

    #[test]
    fn page_index_from_toml() {
        let contents = String::from(
            r#"
+++
date = "2016-04-17"
draft = false
title = """Evaluating Software Design"""
slug = "evaluating-software-design"
tags = ['software development', 'revision', 'design']
banner = ""
aliases = ['/evaluating-software-design/']
+++

Design is iterative
"#,
        );

        let page_index = process_md_toml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_ok());
        let page_index = page_index.unwrap();
        assert_eq!(page_index.title, "Evaluating Software Design");
        assert_eq!(page_index.content, "Design is iterative");
        assert_eq!(page_index.date, "2016-04-17");
        assert_eq!(
            page_index.tags,
            vec!["software development", "revision", "design"]
        );

        // Should be empty as not provided
        assert!(page_index.series.is_empty());
        assert!(page_index.keywords.is_empty());
        assert!(page_index.description.is_empty());
        assert!(page_index.categories.is_empty());
    }

    #[test]
    fn page_index_from_toml_returns_skip_err_when_draft() {
        let contents = String::from(
            r#"
+++
date = "2016-04-17"
draft = true
title = """Evaluating Software Design"""
slug = "evaluating-software-design"
tags = ['software development', 'revision', 'design']
+++

Design is iterative
"#,
        );

        let page_index = process_md_toml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_err());
        // Pattern match error
        match page_index.unwrap_err() {
            OperationResult::Skip(_) => (), // The case where the result is a Skip result succeeds
            _ => panic!("This should fail"), // All other cases fail
        }
    }

    #[test]
    fn page_index_from_toml_returns_parse_err_for_missing_front_matter_fence() {
        let contents = String::from(
            r#"
+++
date = "2016-04-17"
draft = false
title = """Evaluating Software Design"""
slug = "evaluating-software-design"
tags = ['software development', 'revision', 'design']

Design is iterative
"#,
        );

        let page_index = process_md_toml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_err());
        // Pattern match error
        match page_index.unwrap_err() {
            OperationResult::Parse(_) => (), // The case where the result is a Parse result succeeds
            _ => panic!("This should fail"), // All other cases fail
        }
    }

    #[test]
    fn page_index_from_toml_returns_parse_err_for_missing_title_field() {
        let contents = String::from(
            r#"
+++
date = "2016-04-17"
draft = false
slug = "evaluating-software-design"
tags = ['software development', 'revision', 'design']
+++

Design is iterative
"#,
        );

        let page_index = process_md_toml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_err());
        // Pattern match error
        match page_index.unwrap_err() {
            OperationResult::Parse(_) => (), // The case where the result is a Parse result succeeds
            _ => panic!("This should fail"), // All other cases fail
        }
    }

    #[test]
    fn page_index_from_toml_returns_parse_err_for_malformed_toml() {
        let contents = String::from(
            r#"
+++
date: "2016-04-17"
draft = false
title = """Evaluating Software Design"""
slug = "evaluating-software-design"
tags = ['software development', 'revision', 'design']
+++

Design is iterative
"#,
        );

        let page_index = process_md_toml_front_matter(&contents, &build_file_location(), false);
        assert!(page_index.is_err());
        // Pattern match error
        match page_index.unwrap_err() {
            OperationResult::Parse(_) => (), // The case where the result is a Parse result succeeds
            _ => panic!("This should fail"), // All other cases fail
        }
    }
}
