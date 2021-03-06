use docset::{DocSet, SkipResult};
use query::Scorer;
use DocId;
use Score;

/// Creates a `DocSet` that iterator through the intersection of two `DocSet`s.
pub struct Intersection<TDocSet: DocSet> {
    docsets: Vec<TDocSet>,
    finished: bool,
    doc: DocId,
}

impl<TDocSet: DocSet> From<Vec<TDocSet>> for Intersection<TDocSet> {
    fn from(mut docsets: Vec<TDocSet>) -> Intersection<TDocSet> {
        assert!(docsets.len() >= 2);
        docsets.sort_by_key(|docset| docset.size_hint());
        Intersection {
            docsets,
            finished: false,
            doc: 0u32,
        }
    }
}

impl<TDocSet: DocSet> Intersection<TDocSet> {
    /// Returns an array to the underlying `DocSet`s of the intersection.
    /// These `DocSet` are in the same position as the `IntersectionDocSet`,
    /// so that user can access their `docfreq` and `positions`.
    pub fn docsets(&self) -> &[TDocSet] {
        &self.docsets[..]
    }
}

impl<TDocSet: DocSet> DocSet for Intersection<TDocSet> {
    #[allow(never_loop)]
    fn advance(&mut self) -> bool {
        if self.finished {
            return false;
        }

        let mut candidate_doc = self.doc;
        let mut candidate_ord = self.docsets.len();

        'outer: loop {
            for (ord, docset) in self.docsets.iter_mut().enumerate() {
                if ord != candidate_ord {
                    // `candidate_ord` is already at the
                    // right position.
                    //
                    // Calling `skip_next` would advance this docset
                    // and miss it.
                    match docset.skip_next(candidate_doc) {
                        SkipResult::Reached => {}
                        SkipResult::OverStep => {
                            // this is not in the intersection,
                            // let's update our candidate.
                            candidate_doc = docset.doc();
                            candidate_ord = ord;
                            continue 'outer;
                        }
                        SkipResult::End => {
                            self.finished = true;
                            return false;
                        }
                    }
                }
            }

            self.doc = candidate_doc;
            return true;
        }
    }

    fn skip_next(&mut self, target: DocId) -> SkipResult {
        // We optimize skipping by skipping every single member
        // of the intersection to target.
        let mut current_target: DocId = target;
        let mut current_ord = self.docsets.len();

        'outer: loop {
            for (ord, docset) in self.docsets.iter_mut().enumerate() {
                if ord == current_ord {
                    continue;
                }
                match docset.skip_next(current_target) {
                    SkipResult::End => {
                        return SkipResult::End;
                    }
                    SkipResult::OverStep => {
                        // update the target
                        // for the remaining members of the intersection.
                        current_target = docset.doc();
                        current_ord = ord;
                        continue 'outer;
                    }
                    SkipResult::Reached => {}
                }
            }

            self.doc = current_target;
            if target == current_target {
                return SkipResult::Reached;
            } else {
                assert!(current_target > target);
                return SkipResult::OverStep;
            }
        }
    }

    fn doc(&self) -> DocId {
        self.doc
    }

    fn size_hint(&self) -> u32 {
        self.docsets
            .iter()
            .map(|docset| docset.size_hint())
            .min()
            .unwrap_or(0u32)
    }
}

impl<TScorer> Scorer for Intersection<TScorer>
where
    TScorer: Scorer,
{
    fn score(&mut self) -> Score {
        self.docsets.iter_mut().map(Scorer::score).sum()
    }
}

#[cfg(test)]
mod tests {
    use docset::{DocSet, SkipResult};
    use super::Intersection;
    use query::VecDocSet;
    use postings::tests::test_skip_against_unoptimized;

    #[test]
    fn test_intersection() {
        {
            let left = VecDocSet::from(vec![1, 3, 9]);
            let right = VecDocSet::from(vec![3, 4, 9, 18]);
            let mut intersection = Intersection::from(vec![left, right]);
            assert!(intersection.advance());
            assert_eq!(intersection.doc(), 3);
            assert!(intersection.advance());
            assert_eq!(intersection.doc(), 9);
            assert!(!intersection.advance());
        }
        {
            let a = VecDocSet::from(vec![1, 3, 9]);
            let b = VecDocSet::from(vec![3, 4, 9, 18]);
            let c = VecDocSet::from(vec![1, 5, 9, 111]);
            let mut intersection = Intersection::from(vec![a, b, c]);
            assert!(intersection.advance());
            assert_eq!(intersection.doc(), 9);
            assert!(!intersection.advance());
        }
    }

    #[test]
    fn test_intersection_zero() {
        let left = VecDocSet::from(vec![0]);
        let right = VecDocSet::from(vec![0]);
        let mut intersection = Intersection::from(vec![left, right]);
        assert!(intersection.advance());
        assert_eq!(intersection.doc(), 0);
    }

    #[test]
    fn test_intersection_skip() {
        let left = VecDocSet::from(vec![0, 1, 2, 4]);
        let right = VecDocSet::from(vec![2, 5]);
        let mut intersection = Intersection::from(vec![left, right]);
        assert_eq!(intersection.skip_next(2), SkipResult::Reached);
        assert_eq!(intersection.doc(), 2);
    }

    #[test]
    fn test_intersection_skip_against_unoptimized() {
        test_skip_against_unoptimized(
            || {
                let left = VecDocSet::from(vec![4]);
                let right = VecDocSet::from(vec![2, 5]);
                box Intersection::from(vec![left, right])
            },
            vec![0, 2, 4, 5, 6],
        );
        test_skip_against_unoptimized(
            || {
                let mut left = VecDocSet::from(vec![1, 4, 5, 6]);
                let mut right = VecDocSet::from(vec![2, 5, 10]);
                left.advance();
                right.advance();
                box Intersection::from(vec![left, right])
            },
            vec![0, 1, 2, 3, 4, 5, 6, 7, 10, 11],
        );
        test_skip_against_unoptimized(
            || {
                box Intersection::from(vec![
                    VecDocSet::from(vec![1, 4, 5, 6]),
                    VecDocSet::from(vec![1, 2, 5, 6]),
                    VecDocSet::from(vec![1, 4, 5, 6]),
                    VecDocSet::from(vec![1, 5, 6]),
                    VecDocSet::from(vec![2, 4, 5, 7, 8]),
                ])
            },
            vec![0, 1, 2, 3, 4, 5, 6, 7, 10, 11],
        );
    }

    #[test]
    fn test_intersection_empty() {
        let a = VecDocSet::from(vec![1, 3]);
        let b = VecDocSet::from(vec![1, 4]);
        let c = VecDocSet::from(vec![3, 9]);
        let mut intersection = Intersection::from(vec![a, b, c]);
        assert!(!intersection.advance());
    }
}
