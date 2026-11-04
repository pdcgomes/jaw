//! Voting in a poll, as the Mac's app does: Enter on a selected poll, or the palette's "Vote in
//! this poll", opens the palette on its options, each marked as jawd says you chose it, with how
//! many chose it, and a number or Enter votes for one with `poll.vote`, by name, as WhatsApp's apps
//! have it: in a poll of one choice, that option alone, or none when it's your choice already,
//! which takes your vote back; in a poll of several, your choices with it added or taken out, never
//! more than the poll takes. The poll shows your vote once jawd has it, and until then the next
//! choice builds on the vote asked, as the Mac's does. 1 to 6 stay the selected message's quick
//! reactions, as on the Mac.

use super::View;
use super::palette::{Action, Does, Item, Mode, Narrow, Palette};
use crate::data::{Kind, Message, Poll};
use crate::fmt;

/// The options' keys in the picker.
const KEYS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// What choosing option `name` votes for, from your choices `mine`: every option chosen then, by
/// name, in the poll's order, none taking your vote back; or, one more than the poll takes, the
/// most it takes, which jawd would refuse.
pub fn choosing(poll: &Poll, mine: &[String], name: &str) -> Result<Vec<String>, u32> {
    if poll.max <= 1 {
        return Ok(if mine.len() == 1 && mine[0] == name { vec![] } else { vec![name.to_string()] });
    }
    if mine.iter().any(|m| m == name) {
        return Ok(mine.iter().filter(|m| *m != name).cloned().collect());
    }
    if mine.len() >= poll.max as usize {
        return Err(poll.max);
    }
    Ok(poll.options.iter().map(|o| o.name.clone()).filter(|n| n == name || mine.contains(n)).collect())
}

/// The poll a message is, if it can be voted in: not deleted, with the options jawd sent.
pub fn votable(m: &Message) -> Option<&Poll> {
    m.poll.as_deref().filter(|_| m.kind == Kind::Poll && !m.deleted)
}

impl View {
    /// Enter on a selected poll, or the palette's "Vote in this poll": the palette on its options.
    pub(super) fn palette_vote(&mut self, m: &Message) {
        if m.deleted {
            self.say("a deleted poll can't be voted in");
            return;
        }
        if votable(m).is_none() {
            self.say("jawd hasn't sent this poll's options");
            return;
        }
        let mut p = Palette::new();
        p.narrow = Narrow::Actions;
        p.answered = true;
        p.mode = Mode::Vote { message: m.id.clone() };
        self.palette = Some(p);
    }

    /// The picker's rows: the poll's question and what a vote may choose, then each option marked
    /// as jawd says you chose it, with how many chose it and its key, and once you have a vote,
    /// taking it back.
    pub(super) fn vote_items(&self, message: &str) -> Vec<Item> {
        let Some(poll) = self.messages.as_ref().and_then(|w| w.get(message)).and_then(votable) else { return vec![] };
        let said = format!("{} · {}", fmt::fit(&poll.question, 48), poll.rule());
        let mut items = vec![Item::Heading("vote", said)];
        for (i, o) in poll.options.iter().enumerate() {
            let votes = match o.votes {
                0 => "no votes".to_string(),
                1 => "1 vote".into(),
                n => format!("{} votes", fmt::num(n as u64)),
            };
            let mut a = Action::new(o.name.clone(), votes, Does::Vote(message.to_string(), Some(i)));
            if let Some(key) = KEYS.get(i) {
                a = a.key(key);
            }
            let mark = match (o.mine, poll.max > 1) {
                (true, false) => "●",
                (false, false) => "○",
                (true, true) => "■",
                (false, true) => "□",
            };
            a.lead = Some(mark.into());
            items.push(Item::Action(a));
        }
        if poll.options.iter().any(|o| o.mine) {
            items.push(Item::Action(Action::new("Take back my vote", "", Does::Vote(message.to_string(), None))));
        }
        items
    }

    /// A number in the picker: a vote for the option it's the key of.
    pub(super) fn vote_number(&mut self, message: &str, i: usize) {
        let poll = self.messages.as_ref().and_then(|w| w.get(message)).and_then(votable);
        match poll.map_or(0, |p| p.options.len()) {
            n if i < n => self.vote_act(message, Some(i)),
            n => self.say(format!("this poll has {n} options")),
        }
    }

    /// A vote for option `i` of poll `message`, or with `None` your vote taken back, built on the
    /// vote last asked while jawd's row doesn't show it yet. A choice past what the poll takes is
    /// said, and asks jawd nothing. A vote in a poll of one choice closes the picker, and one in a
    /// poll of several leaves it open for the next.
    pub(super) fn vote_act(&mut self, message: &str, option: Option<usize>) {
        let Some(m) = self.messages.as_ref().and_then(|w| w.get(message)).cloned() else { return };
        let Some(poll) = votable(&m) else { return };
        let options = match option {
            None => Ok(vec![]),
            Some(i) => {
                let Some(name) = poll.options.get(i).map(|o| o.name.clone()) else { return };
                let mine = self.votes.get(message).map_or_else(|| poll.mine(), |(_, asked)| asked.clone());
                choosing(poll, &mine, &name)
            }
        };
        let options = match options {
            Ok(options) => options,
            Err(most) => return self.say(format!("this poll takes {most} at most, so take one back first")),
        };
        if option.is_none() || poll.max <= 1 {
            self.palette = None;
        }
        let Some(chat) = self.open.clone() else { return };
        let key = crate::data::key();
        self.jawd.vote(&chat, message, &options, &key);
        self.votes.insert(message.to_string(), (key, options));
    }

    /// Lets go of the votes asked that their polls' rows now show, or whose polls were deleted, so
    /// the next choice goes by jawd's row.
    pub(super) fn votes_shown(&mut self) {
        let Some(w) = self.messages.as_ref() else { return };
        self.votes.retain(|id, (_, asked)| w.get(id).is_none_or(|m| votable(m).is_some_and(|p| p.mine() != *asked)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::PollOption;

    fn poll(max: u32) -> Poll {
        let option = |name: &str| PollOption { name: name.into(), votes: 0, mine: false };
        Poll {
            question: "Onde?".into(),
            options: vec![option("Ramen"), option("Tasca"), option("Pizza")],
            max,
            voters: 0,
        }
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_choice_in_a_poll_of_one_replaces_the_vote_and_the_same_again_takes_it_back() {
        let one = poll(1);
        assert_eq!(choosing(&one, &[], "Tasca"), Ok(names(&["Tasca"])));
        assert_eq!(choosing(&one, &names(&["Ramen"]), "Tasca"), Ok(names(&["Tasca"])));
        assert_eq!(choosing(&one, &names(&["Tasca"]), "Tasca"), Ok(vec![]));
    }

    #[test]
    fn a_choice_in_a_poll_of_several_adds_in_the_polls_order_takes_off_and_stops_at_its_most() {
        let two = poll(2);
        assert_eq!(choosing(&two, &names(&["Pizza"]), "Ramen"), Ok(names(&["Ramen", "Pizza"])));
        assert_eq!(choosing(&two, &names(&["Ramen", "Pizza"]), "Ramen"), Ok(names(&["Pizza"])));
        assert_eq!(choosing(&two, &names(&["Ramen", "Pizza"]), "Tasca"), Err(2));
        assert_eq!(choosing(&poll(3), &names(&["Ramen", "Pizza"]), "Tasca"), Ok(names(&["Ramen", "Tasca", "Pizza"])));
    }
}
