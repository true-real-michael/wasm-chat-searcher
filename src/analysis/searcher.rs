use crate::analysis::deserialization::{deserialize_messages, Message, TextEntity};
use crate::analysis::lemmatizer::Lemmatizer;
use crate::analysis::merge::{MergeAnd, MergeOr};
use crate::analysis::query::{Lexer, Parser, SearchQuery};
use crate::analysis::thread_dsu::ThreadDSU;
use crate::analysis::utils;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text {
    Plain(String),
    Highlight(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSearchResult {
    pub thread_id: u32,
    pub score: u32,
    pub title_text: String,
    pub date_unixtime: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageResult {
    pub message_id: usize,
    pub text: Vec<Text>,
    pub reply_to_text: Option<String>,
}

pub struct Searcher {
    messages: Vec<Message>,
    threads: Vec<Vec<usize>>,
    lemmatizer: Arc<Mutex<Lemmatizer>>,
    thread_index: HashMap<String, Vec<usize>>,
}

impl Searcher {
    pub fn new(lemmatizer: Arc<Mutex<Lemmatizer>>, json: String) -> anyhow::Result<Searcher> {
        let mut thread_dsu = ThreadDSU::new();
        let messages = deserialize_messages(json)?;

        for message in &messages {
            thread_dsu.make_set(message.id);
        }

        for (id, message) in messages.iter().enumerate() {
            if let Some(reply_to) = message.reply_to_message_id {
                thread_dsu.union_sets(reply_to, id); // order is important
            }
        }

        let threads = thread_dsu.get_threads();

        let mut thread_index = HashMap::new();
        for (thread_id, message_ids) in threads.iter().enumerate() {
            let mut used_words = HashSet::new();
            for message_id in message_ids {
                for text_entity in &messages[*message_id].text_entities {
                    if let TextEntity::Lemmatizable(text) = text_entity {
                        text.to_lowercase()
                            .split(|c: char| !c.is_alphanumeric())
                            .filter(|word| word.len() > 3)
                            .map(|word| lemmatizer.lock().unwrap().lemmatize(word))
                            .for_each(|word| {
                                if !used_words.contains(&word) {
                                    thread_index
                                        .entry(word.clone())
                                        .or_insert_with(Vec::new)
                                        .push(thread_id);
                                    used_words.insert(word);
                                }
                            });
                    }
                }
            }
        }
        Ok(Self {
            messages,
            threads,
            lemmatizer,
            thread_index,
        })
    }

    fn find_threads_by_word(&self, word: String) -> Vec<usize> {
        utils::log!("find_threads_by_word({})", word);
        let word = word.to_lowercase();
        let word = self.lemmatizer.lock().unwrap().lemmatize(&word);
        self.thread_index.get(&word).cloned().unwrap_or_default()
    }

    fn find_threads_by_query(&self, query: SearchQuery) -> Vec<usize> {
        match query {
            SearchQuery::Word(word) => self.find_threads_by_word(word),
            SearchQuery::Or((query_left, query_right)) => MergeOr::new(
                self.find_threads_by_query(*query_left).iter(),
                self.find_threads_by_query(*query_right).iter(),
            )
            .copied()
            .collect(),
            SearchQuery::And((query_left, query_right)) => MergeAnd::new(
                self.find_threads_by_query(*query_left).iter(),
                self.find_threads_by_query(*query_right).iter(),
            )
            .copied()
            .collect(),
        }
    }

    pub fn get_query_words(&self, query: String) -> Vec<String> {
        query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.len() > 3)
            .map(|word| self.lemmatizer.lock().unwrap().lemmatize(word))
            .collect()
    }

    pub fn find_threads(&self, query: String) -> anyhow::Result<Vec<ThreadSearchResult>> {
        let query = Parser::new(Lexer::new(&query))?.parse()?;

        let mut result: Vec<ThreadSearchResult> = self
            .find_threads_by_query(query)
            .into_iter()
            .map(|thread_id| {
                let message_id = self.threads[thread_id].first().copied().unwrap();
                let message = &self.messages[message_id];
                ThreadSearchResult {
                    thread_id: thread_id as u32,
                    score: 0,
                    title_text: message.clone().into(),
                    date_unixtime: message.date_unixtime,
                }
            })
            .collect();
        result.sort_by_key(|thread| -(thread.date_unixtime as i32));
        Ok(result)
    }

    pub fn get_thread_messages(&self, thread_id: usize) -> (usize, usize) {
        utils::log!("get_thread_messages({})", thread_id);
        let min_id = self.threads[thread_id].first().copied().unwrap();
        let max_id = self.threads[thread_id].last().copied().unwrap();
        (min_id, max_id)
    }

    pub fn get_message_range(
        &self,
        message_id_min: usize,
        message_id_max: usize,
        query_words: Vec<String>,
    ) -> Vec<MessageResult> {
        if message_id_min > message_id_max {
            return Vec::new();
        }
        self.messages[message_id_min..=message_id_max]
            .iter()
            .map(|message| {
                let reply_to_text = message
                    .reply_to_message_id
                    .map(|reply_to_id| self.messages[reply_to_id].clone().into());
                MessageResult {
                    message_id: message.id,
                    text: self.get_highlighted_text(message.text_entities.clone(), &query_words),
                    reply_to_text,
                }
            })
            .collect()
    }

    fn get_highlighted_text(&self, text: Vec<TextEntity>, query_words: &[String]) -> Vec<Text> {
        text.into_iter()
            .flat_map(|text_entity| match text_entity {
                TextEntity::Lemmatizable(text) => self.highlight_substrings(text, query_words),
                TextEntity::Illemmatizable(text) => vec![Text::Plain(text)],
            })
            .collect()
    }
    fn highlight_substrings(&self, target: String, queries: &[String]) -> Vec<Text> {
        let mut result = Vec::new();
        let mut target = target;
        while !target.is_empty() {
            let next_non_alphanumeric = target.find(|c: char| !c.is_alphanumeric());
            let (word, rest) = match next_non_alphanumeric {
                Some(index) => {
                    let (word, rest) = target.split_at(index);
                    (word.to_string(), rest.to_string())
                }
                None => (target.clone(), "".into()),
            };
            target = rest;
            let lemmatized_word = self
                .lemmatizer
                .lock()
                .unwrap()
                .lemmatize(&word.to_lowercase());
            if queries.contains(&lemmatized_word) {
                result.push(Text::Highlight(word));
            } else {
                result.push(Text::Plain(word));
            }
            while !target.is_empty() && !target.chars().peekable().peek().unwrap().is_alphanumeric()
            {
                result.push(Text::Plain(target.chars().next().unwrap().to_string()));
                target = target.chars().skip(1).collect();
            }
        }
        result
    }
}
