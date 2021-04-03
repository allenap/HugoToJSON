use crate::constants::*;
use crate::file_location::FileLocation;
use crate::operation_result::*;

#[derive(Serialize, Debug, PartialEq)]
/// Represents a summary of an individual document/post in the contents directory.
pub struct PageIndex {
    /// Title of the document from frontmatter
    pub title: String,
    /// href of the document, constructed from either frontmatter if it exists or relative path info
    pub href: String,
    /// date of the document from frontmatter
    pub date: String,
    /// contents of the document with markdown stripped away
    pub content: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    /// Description of the document from frontmatter
    pub description: String,
    /// Categories of the document from frontmatter
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Series of the document from frontmatter
    pub series: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Tags of the document from frontmatter
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Keywords of the document from frontmatter
    pub keywords: Vec<String>,
    /// Is this a draft?
    pub draft: bool,
}

// TODO: Make this use a builder pattern.
impl PageIndex {
    #![allow(clippy::too_many_arguments)]
    /// Creates a new Page Index from the parameters read from the frontmatter of the document.
    /// It will also try and construct a href to the document using either data in the frontmatter or based on the file location.
    /// # Errors
    /// This will fail if a document doesn't contain a title or a date or they can't be read from the frontmatter.
    pub fn new(
        title: Option<&str>,
        slug: Option<&str>,
        date: Option<&str>,
        description: Option<&str>,
        categories: Vec<String>,
        series: Vec<String>,
        tags: Vec<String>,
        keywords: Vec<String>,
        content: String,
        file_location: &FileLocation,
        url: Option<&str>,
        draft: bool,
    ) -> Result<Self, OperationResult> {
        let title = title
            .ok_or_else(|| {
                OperationResult::Parse(ParseError::new(
                    &file_location.absolute_path,
                    "Could not read title from front matter",
                ))
            })?
            .trim()
            .to_owned();

        let date = date
            .ok_or_else(|| {
                OperationResult::Parse(ParseError::new(
                    &file_location.absolute_path,
                    "Could not read date from front matter",
                ))
            })?
            .trim()
            .to_owned();

        let description = description.unwrap_or("").to_owned();

        let href = build_href(slug, url, file_location);

        Ok(Self {
            title,
            date,
            description,
            categories,
            tags,
            series,
            keywords,
            href,
            content,
            draft,
        })
    }
}

/// Tries a builds a href to the document using either slug and urls provided by frontmatter or using path info.
fn build_href(
    possible_slug: Option<&str>,
    possible_url: Option<&str>,
    file_location: &FileLocation,
) -> String {
    if let Some(url) = possible_url {
        return url.to_string();
    }

    let relative_part = match file_location.relative_directory_to_content.as_ref() {
        "" => EMPTY_STRING.to_owned(),
        _ => [FORWARD_SLASH, &file_location.relative_directory_to_content].concat(),
    };

    if let Some(slug) = possible_slug {
        return [&relative_part, FORWARD_SLASH, slug, FORWARD_SLASH].concat();
    }

    [
        &relative_part,
        FORWARD_SLASH,
        &file_location.file_stem.to_lowercase(),
        FORWARD_SLASH,
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_file_location() -> FileLocation {
        FileLocation {
            extension: String::from("md"),
            relative_directory_to_content: String::from("post"),
            absolute_path: String::from("/home/blog/content/post/example.md"),
            file_stem: String::from("example"),
            file_name: String::from("example.md"),
        }
    }

    #[test]
    fn constructs_valid_href_with_slug() {
        let title = Some("Title");
        let slug = Some("my-example-post");
        let date = Some("2018-01-01");
        let description = Some("An example description");
        let categories = Vec::new();
        let tags = Vec::new();
        let keywords = Vec::new();
        let series = Vec::new();
        let content = "A lot of content".to_owned();
        let url = None;

        let page_index = PageIndex::new(
            title,
            slug,
            date,
            description,
            categories,
            series,
            tags,
            keywords,
            content,
            &build_file_location(),
            url,
            false,
        );
        assert!(page_index.is_ok());
        assert_eq!(page_index.unwrap().href, "/post/my-example-post/")
    }

    #[test]
    fn constructs_valid_href_with_url() {
        let title = Some("Title");
        let slug = None;
        let date = Some("2018-01-01");
        let description = Some("An example description");
        let categories = Vec::new();
        let tags = Vec::new();
        let keywords = Vec::new();
        let series = Vec::new();
        let content = "A lot of content".to_owned();
        let url = Some("deep/nested/post/my-example-post/");

        let page_index = PageIndex::new(
            title,
            slug,
            date,
            description,
            categories,
            series,
            tags,
            keywords,
            content,
            &build_file_location(),
            url,
            false,
        );
        assert!(page_index.is_ok());
        assert_eq!(
            page_index.unwrap().href,
            "deep/nested/post/my-example-post/"
        )
    }

    #[test]
    fn constructs_correct_href_without_slug_or_url() {
        let title = Some("Title");
        let slug = None;
        let date = Some("2018-01-01");
        let description = Some("An example description");
        let categories = Vec::new();
        let tags = Vec::new();
        let keywords = Vec::new();
        let series = Vec::new();
        let content = "A lot of content".to_owned();
        let url = None;

        let page_index = PageIndex::new(
            title,
            slug,
            date,
            description,
            categories,
            series,
            tags,
            keywords,
            content,
            &build_file_location(),
            url,
            false,
        );
        assert!(page_index.is_ok());
        assert_eq!(page_index.unwrap().href, "/post/example/")
    }

    #[test]
    fn href_for_no_slug_or_url_lowers_filename() {
        let title = Some("Title");
        let slug = None;
        let date = Some("2018-01-01");
        let description = Some("An example description");
        let categories = Vec::new();
        let tags = Vec::new();
        let keywords = Vec::new();
        let series = Vec::new();
        let content = "A lot of content".to_owned();
        let url = None;
        let mut file_location = build_file_location();
        file_location.file_stem = file_location.file_stem.to_uppercase();

        let page_index = PageIndex::new(
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
            false,
        );
        assert!(page_index.is_ok());
        assert_eq!(page_index.unwrap().href, "/post/example/")
    }

    #[test]
    fn no_title_returns_err() {
        let title = None;
        let slug = Some("my-example-post");
        let date = Some("2018-01-01");
        let description = Some("An example description");
        let categories = Vec::new();
        let tags = Vec::new();
        let keywords = Vec::new();
        let series = Vec::new();
        let content = "A lot of content".to_owned();
        let url = None;

        assert!(PageIndex::new(
            title,
            slug,
            date,
            description,
            categories,
            series,
            tags,
            keywords,
            content,
            &build_file_location(),
            url,
            false,
        )
        .is_err());
    }

    #[test]
    fn no_date_returns_err() {
        let title = Some("Title");
        let slug = Some("my-example-post");
        let date = None;
        let description = Some("An example description");
        let categories = Vec::new();
        let tags = Vec::new();
        let keywords = Vec::new();
        let series = Vec::new();
        let content = "A lot of content".to_owned();
        let url = None;

        assert!(PageIndex::new(
            title,
            slug,
            date,
            description,
            categories,
            series,
            tags,
            keywords,
            content,
            &build_file_location(),
            url,
            false,
        )
        .is_err());
    }
}
