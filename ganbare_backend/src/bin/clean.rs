extern crate ganbare_backend;
#[macro_use]
extern crate clap;
extern crate dotenv;
extern crate mime;
extern crate unicode_normalization;
extern crate tempdir;
#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate lazy_static;
extern crate rand;
extern crate regex;
extern crate time;
extern crate crypto;
extern crate r2d2;

use ganbare_backend::*;
use std::path::PathBuf;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

lazy_static! {

    static ref DATABASE_URL : String = {
        dotenv::dotenv().ok();
        std::env::var("GANBARE_DATABASE_URL")
            .expect(
            "GANBARE_DATABASE_URL must be set (format: postgres://username:password@host/dbname)"
            )
    };

    pub static ref AUDIO_DIR : PathBuf = {
        dotenv::dotenv().ok();
        PathBuf::from(std::env::var("GANBARE_AUDIO_DIR")
            .unwrap_or_else(|_| "../audio".into()))
    };

    pub static ref IMAGE_DIR : PathBuf = {
        dotenv::dotenv().ok();
        PathBuf::from(std::env::var("GANBARE_IMAGES_DIR")
            .unwrap_or_else(|_| "../images".into()))
    };
}

pub fn tidy_span_and_br_tags() -> Result<Vec<String>> {
    use ganbare_backend::schema::{words, question_answers};
    let conn = db::connect(&*DATABASE_URL).unwrap();

    let mut logger = vec![];

    let r2 = regex::Regex::new(r#"<span .*?>"#).expect("<- that is a valid regex there");
    let r3 = r#"</span>"#;
    let r4 = regex::Regex::new(r#"<br .*?>"#).expect("<- that is a valid regex there");


    let words: Vec<Word> =
        words::table.filter(words::explanation.like("%span%").or(words::explanation.like("%<br %")))
            .get_results(&conn)?;

    for mut w in words {
        let before = format!("{:?}", w);

        w.explanation = r2.replace_all(&w.explanation, "").into_owned();
        w.explanation = w.explanation.replace(r3, "");
        w.explanation = r4.replace_all(&w.explanation, "<br>").into_owned();

        logger.push(format!("Tidied a span/br tag!\n{}\n→\n{:?}\n", before, w));

        let _: Word = w.save_changes(&conn)?;
    }

    let answers: Vec<Answer> =
        question_answers::table.filter(question_answers::answer_text.like("%span%")
                .or(question_answers::answer_text.like("%<br %")))
            .get_results(&conn)?;

    for mut a in answers {
        let before = format!("{:?}", a);

        a.answer_text = r2.replace_all(&a.answer_text, "").into_owned();
        a.answer_text = a.answer_text.replace(r3, "");
        a.answer_text = r4.replace_all(&a.answer_text, "<br>").into_owned();

        logger.push(format!("Tidied a span/br tag!\n{}\n→\n{:?}\n", before, a));

        let _: Answer = a.save_changes(&conn)?;
    }

    Ok(logger)
}

pub fn outbound_urls_to_inbound() -> Result<Vec<String>> {
    use ganbare_backend::schema::{words, question_answers};
    use ganbare_backend::manage::sanitize_links;

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let mut logger = vec![];

    let words: Vec<Word> = words::table
        .filter(words::explanation.like("%http://%").or(words::explanation.like("%https://%")))
        .get_results(&conn)?;

    for mut w in words {
        let before = format!("{:?}", w);
        w.explanation = sanitize_links(&w.explanation, &*IMAGE_DIR)?;
        logger.push(format!("Converted an outbound image link to inbound!\n{}\n→\n{:?}\n",
                            before,
                            w));

        let _: Word = w.save_changes(&conn)?;
    }

    let answers: Vec<Answer> =
        question_answers::table.filter(question_answers::answer_text.like("%http://%")
                .or(question_answers::answer_text.like("%https://%")))
            .get_results(&conn)?;

    for mut a in answers {
        let before = format!("{:?}", a);
        a.answer_text = sanitize_links(&a.answer_text, &*IMAGE_DIR)?;
        logger.push(format!("Converted an outbound image link to inbound!\n{}\n→\n{:?}\n",
                            before,
                            a));

        let _: Answer = a.save_changes(&conn)?;
    }

    Ok(logger)
}

fn normalize_unicode() {

    let config = r2d2::Config::default();
    let manager = ConnManager::new(DATABASE_URL.as_str());
    let pool = r2d2::Pool::new(config, manager).expect("Failed to create pool.");
    let pooled_conn = pool.get().unwrap();

    let bundles = audio::get_all_bundles(&pooled_conn).unwrap();


    let conn = &*pooled_conn;

    for (mut b, _) in bundles {
        let cleaned_name = b.listname.nfc().collect::<String>();
        if cleaned_name != b.listname {
            println!("Non-normalized unicode found: {:?}", b);
            b.listname = cleaned_name;
            let _: AudioBundle = b.save_changes(conn).unwrap();
        }
    }

    let words: Vec<Word> = schema::words::table.get_results(&*conn).unwrap();

    for mut w in words {
        let cleaned_word = w.word.nfc().collect::<String>();
        if cleaned_word != w.word {
            println!("Non-normalized unicode found: {:?}", w);
            w.word = cleaned_word;
            let _: Word = w.save_changes(conn).unwrap();
        }
    }
    let skills: Vec<SkillNugget> = schema::skill_nuggets::table.get_results(&*conn).unwrap();

    for mut s in skills {
        let cleaned_skill = s.skill_summary.nfc().collect::<String>();
        if cleaned_skill != s.skill_summary {
            println!("Non-normalized unicode found: {:?}", s);
            s.skill_summary = cleaned_skill;
            let _: SkillNugget = s.save_changes(conn).unwrap();
        }
    }
}

fn clean_unused_audio() {
    let config = r2d2::Config::default();
    let manager = ConnManager::new(DATABASE_URL.as_str());
    let pool = r2d2::Pool::new(config, manager).expect("Failed to create pool.");
    let pooled_conn = pool.get().unwrap();

    let fs_files = std::fs::read_dir(&*AUDIO_DIR).unwrap();

    let db_files: HashSet<String> =
        audio::get_all_files(&pooled_conn).unwrap().into_iter().map(|f| f.0).collect();

    let mut trash_dir = AUDIO_DIR.clone();
    trash_dir.push("trash");

    for f in fs_files {
        let f = f.unwrap();
        let f_name = f.file_name();
        if !db_files.contains(f_name.to_str().unwrap()) && f_name != *"trash" {
            trash_dir.push(&f_name);
            info!("Moving a unneeded file {:?} to the trash directory.",
                  &f_name);
            std::fs::rename(f.path(), &trash_dir)
                .expect("Create \"trash\" directory for cleaning up!");
            trash_dir.pop();
        }
    }

}

use regex::Regex;

lazy_static! {

    static ref IMG_REGEX: Regex = Regex::new(r#"<img[^>]* src="[^"]*/([^"]*)"[^>]*>"#)
        .expect("<- that is a valid regex there");

}

fn clean_unused_images() {
    use ganbare_backend::schema::{question_answers, words};

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let fs_files = std::fs::read_dir(&*IMAGE_DIR).expect(&format!("Not found: {:?}", &*IMAGE_DIR));

    let mut db_files: HashSet<String> = HashSet::new();

    let words: Vec<Word> = words::table.filter(words::explanation.like("%<img%"))
        .get_results(&conn)
        .unwrap();

    for w in words {

        for img_match in IMG_REGEX.captures_iter(&w.explanation) {
            let img = img_match.get(1).expect("The whole match won't match without this submatch.").as_str();
            db_files.insert(img.to_string());
        }
    }

    let answers: Vec<Answer> =
        question_answers::table.filter(question_answers::answer_text.like("%<img%"))
            .get_results(&conn)
            .unwrap();

    for a in answers {
        for img_match in IMG_REGEX.captures_iter(&a.answer_text) {
            let img = img_match.get(1).expect("The whole match won't match without this submatch.").as_str();
            db_files.insert(img.to_string());
        }
    }

    let mut trash_dir = IMAGE_DIR.clone();
    trash_dir.push("trash");

    for f in fs_files {
        let f = f.unwrap();
        let f_name = f.file_name();
        if !db_files.contains(f_name.to_str().unwrap()) && f_name != *"trash" {
            trash_dir.push(&f_name);
            info!("Moving a unneeded file {:?} to the trash directory.",
                  &f_name);
            std::fs::rename(f.path(), &trash_dir)
                .expect("Create \"trash\" directory for cleaning up!");
            trash_dir.pop();
        }
    }
}

lazy_static! {

    static ref BR_IMG_REGEX: Regex = Regex::new(r#"([^>])(<img[^>]* src="[^"]+"[^>]*>)"#)
        .expect("<- that is a valid regex there");

}

fn add_br_between_images_and_text() {
    use ganbare_backend::schema::{question_answers, words};

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let words: Vec<Word> = words::table.filter(words::explanation.like("%<img%"))
        .get_results(&conn)
        .unwrap();

    for mut w in words {
        let new_text = BR_IMG_REGEX.replace_all(&w.explanation, "$1<br>$2").into_owned();
        if new_text != w.explanation {
            println!("Added a br tag:\n{:?}\n→\n{:?}\n",
                     w.explanation,
                     new_text);
            w.explanation = new_text;
            let _: Word = w.save_changes(&conn).unwrap();
        }
    }

    let answers: Vec<Answer> =
        question_answers::table.filter(question_answers::answer_text.like("%<img%"))
            .get_results(&conn)
            .unwrap();

    for mut a in answers {
        let new_text = BR_IMG_REGEX.replace_all(&a.answer_text, "$1<br>$2").into_owned();
        if new_text != a.answer_text {
            println!("Added a br tag:\n{:?}\n→\n{:?}\n",
                     a.answer_text,
                     new_text);
            a.answer_text = new_text;
            let _: Answer = a.save_changes(&conn).unwrap();
        }
    }
}

fn fix_skill_names() {
    use std::io::Read;
    use schema::skill_nuggets;

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let mut cleanup_str = String::with_capacity(300);
    std::fs::File::open("src/bin/skill_cleanup.txt").unwrap().read_to_string(&mut cleanup_str)
        .expect("Why can't it read to a string?");
    let cleanup = cleanup_str.lines().map(|l| {
        let words = l.split_at(l.find('\t').unwrap());
        (words.0, &words.1[1..])
    });

    for (from, to) in cleanup {

        let skill: Option<SkillNugget> = skill_nuggets::table
            .filter(skill_nuggets::skill_summary.eq(from))
            .get_result(&conn)
            .optional()
            .expect("Shoot!");

        if let Some(mut skill) = skill {
            println!("{} → {}", from, to);
            skill.skill_summary = to.to_owned();
            let _: SkillNugget = skill.save_changes(&conn).expect("Crapshoot!");
        }

    }
}

fn merge_redundant_skills() {
    use schema::{words, quiz_questions, exercises, skill_nuggets};
    use diesel::expression::dsl::*;

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let originals: Vec<(i32, i64, String)> =
        sql::<(
        diesel::types::Integer,
        diesel::types::BigInt,
        diesel::types::Text,
        )>(r###"
SELECT MIN(id), COUNT(id), skill_summary FROM skill_nuggets GROUP BY skill_summary HAVING COUNT(id) > 1;
"###).get_results(&conn).expect("DB error");

    for o in originals {
        let original_id = o.0;
        let original_summary = &o.2;

        let dupes: Vec<SkillNugget> = skill_nuggets::table
            .filter(skill_nuggets::skill_summary.eq(original_summary).and(skill_nuggets::id.ne(original_id)))
            .get_results(&conn).expect("DB error");

        for d in dupes {
            println!("Going to remove {:?}, replacing with {:?}", d, o);
            diesel::update(words::table.filter(words::skill_nugget.eq(d.id)))
                .set(words::skill_nugget.eq(original_id))
                .execute(&conn)
                .expect("DB error");
            diesel::update(quiz_questions::table.filter(quiz_questions::skill_id.eq(d.id)))
                .set(quiz_questions::skill_id.eq(original_id))
                .execute(&conn)
                .expect("DB error");
            diesel::update(exercises::table.filter(exercises::skill_id.eq(d.id)))
                .set(exercises::skill_id.eq(original_id))
                .execute(&conn)
                .expect("DB error");
            diesel::delete(skill_nuggets::table.filter(skill_nuggets::id.eq(d.id)))
                .execute(&conn)
                .expect("DB errer");
        }
    }
}

fn add_audio_file_hashes() {
    use schema::audio_files;

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let all_hashless_audio_files: Vec<AudioFile> = audio_files::table
        .filter(audio_files::file_sha2.is_null())
        .get_results(&conn).unwrap();

    for AudioFile{ id, file_path, .. } in all_hashless_audio_files {

        let hash = audio::audio_file_hash(&file_path, &*AUDIO_DIR).unwrap();

        let f : Option<AudioFile> = diesel::update(audio_files::table
                .filter(audio_files::file_path.eq(&file_path)))
            .set(audio_files::file_sha2.eq(&hash[..]))
            .get_result(&conn)
            .optional()
            .or_else(|_| {
                use schema::pending_items;

                let existing: AudioFile = audio_files::table
                    .filter(audio_files::file_sha2.eq(&hash[..])).get_result(&conn).unwrap();
                println!("Hash/file already exists! Bundle: {} Existing: {} {} New: {} {}",
                    existing.bundle_id, existing.id, existing.file_path, id, &file_path);
                println!("Deleting the newer one.");
                let updated = diesel::update(
                        pending_items::table
                            .filter(pending_items::audio_file_id.eq(id))
                    )
                    .set(pending_items::audio_file_id.eq(existing.id))
                    .execute(&conn).expect("Couldn't update!");
                let deleted = diesel::delete(
                        audio_files::table
                            .filter(audio_files::id.eq(id))
                    )
                    .execute(&conn).expect("Couldn't delete!");

                println!("Deleted audio_files rows: {} Updated pending_items rows: {}", deleted, updated);
                Ok::<_, errors::Error>(None)
            }).unwrap();

        if f.is_some() {
            println!("Set hash: {}", &file_path);
        }
    }
}

fn check_skill_levels() {
    use schema::{quiz_questions, exercises};

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let questions: Vec<QuizQuestion> = quiz_questions::table
        .filter(quiz_questions::skill_level.eq(1))
        .get_results(&conn).unwrap();

    for mut q in questions {
        println!("Raising a skill level of a question from 1 → 2. {:?}", q);
        q.skill_level = 2;
        let _: QuizQuestion = q.save_changes(&conn).unwrap();
    }

    let exercises: Vec<Exercise> = exercises::table
        .filter(exercises::skill_level.eq(1))
        .get_results(&conn).unwrap();

    for mut e in exercises {
        println!("Raising a skill level of a exercise from 1 → 2. {:?}", e);
        e.skill_level = 2;
        let _: Exercise = e.save_changes(&conn).unwrap();
    }
}

fn check_priority_levels() {
    use schema::{words, skill_nuggets};

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let priority_skills: Vec<i32> = words::table
        .inner_join(skill_nuggets::table)
        .filter(words::skill_level.ge(5))
        .select(skill_nuggets::id)
        .get_results(&conn).unwrap();

    for skill_id in priority_skills {

        let priority_words: Vec<Word> = diesel::update(
            words::table
                .filter(words::skill_nugget.eq(skill_id).and(words::priority.lt(2)))
            )
            .set(words::priority.eq(2))
            .get_results(&conn).unwrap();

        for word in priority_words {
            println!("Raised a priority level of a word {:?}", word);
        }
    }

}

fn replace_images() {
    use schema::{words, question_answers};

    let new_images = std::fs::read_dir("src/bin/image_cleanup").unwrap();
    let mut image_names = vec![];

    for f in new_images {
        let fname = f.unwrap().file_name();
        let fname = fname.to_str().unwrap();
        image_names.push((fname[0..fname.len()-4].to_owned(), fname[fname.len()-4..fname.len()].to_owned()));
    }

    let conn = db::connect(&*DATABASE_URL).unwrap();

    let words: Vec<Word> = words::table.filter(words::explanation.like("%<img%"))
        .get_results(&conn)
        .unwrap();

    for mut w in words {
        for &(ref i, ref ext) in &image_names {
            if w.explanation.contains(i) {
                let stitched = {
                    let mut pieces = w.explanation.splitn(2, i);
                    let before = pieces.next().unwrap();
                    let ext_after = pieces.next().unwrap();
                    let after = ext_after.splitn(2, '"').skip(1).next().unwrap();
                    format!("{}{}{}\"{}", before, i, ext, after)
                };
                if w.explanation != stitched {
                    println!("{} → {}", w.explanation, stitched);
                    w.explanation = stitched;
                    let _: Word = w.save_changes(&conn).unwrap();
                }
            }
        }
    }

    let answers: Vec<Answer> =
        question_answers::table.filter(question_answers::answer_text.like("%<img%"))
            .get_results(&conn)
            .unwrap();

    for mut a in answers {
        for &(ref i, ref ext) in &image_names {
            if a.answer_text.contains(i) {
                let stitched = {
                    let mut pieces = a.answer_text.splitn(2, i);
                    let before = pieces.next().unwrap();
                    let ext_after = pieces.next().unwrap();
                    let after = ext_after.splitn(2, '"').skip(1).next().unwrap();
    
                    format!("{}{}{}\"{}", before, i, ext, after)
                };
                if a.answer_text != stitched {
                    println!("{} → {}", a.answer_text, stitched);
                    a.answer_text = stitched;
                    let _: Answer = a.save_changes(&conn).unwrap();
                }

            }
        }
    }

    let new_images = std::fs::read_dir("src/bin/image_cleanup").unwrap();

    for f in new_images {
        let fname = f.unwrap().file_name();
        let mut old_path = PathBuf::from("src/bin/image_cleanup");
        let mut new_path = IMAGE_DIR.to_owned();
        old_path.push(&fname);
        new_path.push(&fname);
        println!("{:?} → {:?}", old_path, new_path);
        std::fs::rename(old_path, new_path).unwrap();
    }

}

fn main() {
    use clap::*;

    env_logger::init().unwrap();
    info!("Starting.");

    App::new("ganba.re audio cleaning tool").version(crate_version!());


    for line in outbound_urls_to_inbound().unwrap() {
        println!("{}", line);
    }

    for line in tidy_span_and_br_tags().unwrap() {
        println!("{}", line);
    }

    println!("Clean unused audio and move to trash.");
    clean_unused_audio();
    println!("Clean unused images and move to trash.");
    clean_unused_images();
    println!("Normalize Unicode to Canonical Composition Form.");
    normalize_unicode();
    println!("Add <br> between images and text.");
    add_br_between_images_and_text();
    println!("Fix skill names (remove unrelated suffixes etc. according to src/bin/skill_cleanup.txt)");
    fix_skill_names();
    println!("Add audio file hashes for files that are still missing them.");
    add_audio_file_hashes();
    println!("Fix skill levels (questions and exercises ought to have at least skill level 2).");
    check_skill_levels();
    println!("Fix priority levels (words that are accompanied by sentences ought to have higher priority levels).");
    check_priority_levels();
    println!("Merge redundant skills");
    merge_redundant_skills();
    println!("Replace oversized images");
    replace_images();
}
