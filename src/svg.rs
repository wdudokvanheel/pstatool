use crate::model::{ClocData, Language};
use once_cell::sync::Lazy;
use sqlx::Error;
use std::collections::HashMap;

static LANGUAGE_COLORS: Lazy<HashMap<String, String>> = Lazy::new(|| {
    let yaml_str = include_str!("../assets/langs.yml");
    load_language_colors(yaml_str)
});

#[derive(Debug)]
pub struct SvgTemplateData {
    total_lines: u64,
    total_files: u64,
    bar: String,
    left_block: String,
    right_block: String,
}

pub fn cloc_to_svg_template_data(cloc: &ClocData) -> SvgTemplateData {
    let default_color = "#cccccc";
    let total_loc: u64 = cloc
        .languages
        .values()
        .map(|stats| stats.total_lines())
        .sum();
    let total_files: u64 = cloc.languages.values().map(|stats| stats.n_files).sum();

    if total_loc == 0 {
        return SvgTemplateData {
            total_lines: 0,
            total_files: 0,
            bar: "<svg><!-- No code found --></svg>".to_string(),
            left_block: String::new(),
            right_block: String::new(),
        };
    }

    let mut lang_data: Vec<(String, u64, f64, f64)> = cloc
        .languages
        .iter()
        .map(|(lang, stats)| {
            let pct = (stats.total_lines() as f64 / total_loc as f64) * 100.0;
            let width = (pct / 100.0) * 250.0;
            (lang.clone(), stats.total_lines(), pct, width)
        })
        .collect();

    lang_data.sort_by(|a, b| b.1.cmp(&a.1));
    lang_data.truncate(6);

    let mut rects = String::new();
    let mut cumulative_x = 0.0;
    for (lang, _code, _pct, width) in &lang_data {
        let default_color_owned = default_color.to_string();
        let color = LANGUAGE_COLORS.get(lang).unwrap_or(&default_color_owned);
        rects.push_str(&format!(
            r#"<rect mask="url(#rect-mask)" x="{:.2}" y="0" width="{:.2}" height="8" fill="{}"/>"#,
            cumulative_x, width, color
        ));
        cumulative_x += width;
    }

    let mut left_labels = Vec::new();
    let mut right_labels = Vec::new();

    for (i, (lang, _code, pct, _width)) in lang_data.iter().enumerate() {
        let default_color_owned = default_color.to_string();
        let color = LANGUAGE_COLORS.get(lang).unwrap_or(&default_color_owned);
        let delay = 450 + (i as u32 % 3) * 150;
        let label = format!(
            r#"<g class="stagger" style="animation-delay: {}ms">
    <circle cx="5" cy="6" r="5" fill="{}"/>
    <text x="15" y="10" class="lang-name">{} {:.2}%</text>
</g>"#,
            delay, color, lang, pct
        );
        if i % 2 == 0 {
            left_labels.push(label);
        } else {
            right_labels.push(label);
        }
    }

    let left_group: String = left_labels
        .into_iter()
        .enumerate()
        .map(|(i, label)| format!(r#"<g transform="translate(0, {})">{}</g>"#, i * 25, label))
        .collect::<Vec<_>>()
        .join("\n");

    let right_group: String = right_labels
        .into_iter()
        .enumerate()
        .map(|(i, label)| format!(r#"<g transform="translate(0, {})">{}</g>"#, i * 25, label))
        .collect::<Vec<_>>()
        .join("\n");

    SvgTemplateData {
        total_lines: total_loc,
        total_files: total_files,
        bar: rects,
        left_block: left_group,
        right_block: right_group,
    }
}

pub fn generate_svg(project_name: &str, cloc: &ClocData) -> Result<String, Error> {
    let data = cloc_to_svg_template_data(&cloc);

    let template = include_str!("../assets/template.svg");

    let subheader = format!(
        "{} lines of code in {} files",
        data.total_lines, data.total_files
    );
    let header = format!("Stats for {}", project_name);

    let svg_content = template
        .replace("#header#", &header)
        .replace("#subheader#", &subheader)
        .replace("#bar_rects#", &data.bar)
        .replace("#left_block#", &data.left_block)
        .replace("#right_block#", &data.right_block);

    Ok(svg_content)
}

pub fn load_language_colors(yaml_str: &str) -> HashMap<String, String> {
    let parsed: HashMap<String, Language> =
        serde_yaml::from_str(yaml_str).expect("Failed to parse YAML");

    parsed
        .into_iter()
        .filter_map(|(key, lang)| lang.color.map(|color| (key, color)))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::Path;
    use crate::run_cloc;
    use crate::svg::{generate_svg, load_language_colors};

    #[test]
    fn test_get_lang_color() {
        let yaml = include_str!("../assets/langs.yml");
        let map = load_language_colors(&yaml);
        assert!(map.contains_key("Rust"));
        assert!(map.contains_key("Swift"));
        println!("{:?}", map);
    }

    #[test]
    fn test_svg_gen() {
        let ignored = [
            "target",
            ".idea",
            "*.framework",
            "*.xcodeproj",
            "GStreamer.framework",
            "assets",
            "pkg",
        ];

        let dest = Path::new("/Users/wesley/workspace/chip8/");
        let result = run_cloc(dest, &ignored).unwrap();

        let svg = generate_svg("SleepStream", &result);
        assert!(svg.is_ok());
        let svg_content = svg.unwrap();

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("test.svg")
            .expect("Unable to create or open file");

        file.write_all(svg_content.as_bytes())
            .expect("Unable to write data");
    }
}
