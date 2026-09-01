#!/usr/bin/env python3
"""Mint one example sentence per word for the 65 words blocking release."""

import json
import os
import sys
import urllib.request

API = os.environ.get("MORPHO_API", "http://127.0.0.1:30012")
SOURCE = "manual"

WORDS = {
    1966: ("acceptance", "Her quiet acceptance of the outcome surprised everyone in the room."),
    2096: ("analytic", "The professor favored an analytic approach to solving differential equations."),
    2616: ("comparative", "A comparative study of the two languages revealed unexpected similarities."),
    2692: ("consequently", "The bridge collapsed during the flood; consequently, all traffic was rerouted through the valley."),
    2713: ("consultant", "The firm hired a consultant to restructure their supply chain operations."),
    2921: ("descendant", "Every descendant of the original settlers received a share of the land trust."),
    2924: ("designate", "The committee will designate a spokesperson before the press conference begins."),
    2941: ("detect", "Trained dogs can detect trace amounts of explosives hidden inside luggage."),
    3141: ("employer", "A responsible employer provides health insurance and fair working conditions."),
    3183: ("equator", "Temperatures near the equator remain high throughout the entire year."),
    3231: ("exemplify", "These ruins exemplify the architectural brilliance of the ancient civilization."),
    3314: ("fearful", "The child grew fearful as the thunder rolled closer across the darkening sky."),
    3475: ("glamor", "Old Hollywood's glamor was built on careful lighting and elaborate costume design."),
    3674: ("immense", "The sheer immense scale of the canyon left every visitor speechless."),
    3716: ("indignation", "Public indignation over the corruption scandal forced three officials to resign."),
    3723: ("industrialize", "The government planned to industrialize the rural provinces within a single decade."),
    3746: ("inhibit", "High cortisol levels can inhibit the immune system's ability to fight infection."),
    3749: ("initiative", "She launched a community initiative to provide free tutoring for underprivileged students."),
    3867: ("kidnap", "The film depicts a plot to kidnap a foreign diplomat during a state banquet."),
    4119: ("mobilize", "Relief agencies rushed to mobilize supplies before the second wave of flooding arrived."),
    4173: ("namely", "One factor determines long-term success, namely the willingness to adapt under pressure."),
    4188: ("negligible", "The difference in weight between the two samples was negligible and fell within measurement error."),
    4202: ("nominal", "The membership fee is nominal, just enough to cover printing and postage."),
    4210: ("notify", "The hospital will notify the family as soon as the surgery is complete."),
    4336: ("overwhelming", "The evidence against the defendant was so overwhelming that deliberation lasted only an hour."),
    4423: ("periodical", "She found the original article in a dusty periodical from nineteen forty-seven."),
    4431: ("persevere", "Despite repeated failures, the researchers chose to persevere with the experiment."),
    4432: ("persist", "Symptoms may persist for several weeks after the initial infection has cleared."),
    4536: ("preceding", "All data from the preceding quarter must be reviewed before filing the annual report."),
    4560: ("preside", "The chief justice will preside over the impeachment trial starting next Monday."),
    4588: ("probability", "Statistical models estimate the probability of a magnitude-seven earthquake within fifty years."),
    4602: ("proficiency", "Candidates must demonstrate proficiency in at least two programming languages."),
    4613: ("promising", "Early clinical trials produced promising results for the new vaccine candidate."),
    4828: ("requirement", "Fluency in Mandarin is a strict requirement for this diplomatic posting."),
    4830: ("resemblance", "The resemblance between the twins was so strong that even their parents confused them."),
    4858: ("resultant", "The resultant mixture turned a deep violet when exposed to ultraviolet light."),
    5233: ("statistical", "A statistical analysis of the survey data confirmed the initial hypothesis."),
    5328: ("superiority", "The general's strategy relied on achieving air superiority before launching the ground assault."),
    5420: ("testify", "Three eyewitnesses agreed to testify in court about what they had seen that evening."),
    5690: ("administrative", "The administrative burden of the new regulations frustrated small-business owners across the state."),
    5694: ("allocation", "Fair allocation of resources remains the central challenge in disaster response."),
    5761: ("embarrassment", "His public embarrassment over the gaffe only deepened when the video went viral."),
    5774: ("exertion", "After hours of physical exertion in the summer heat, the crew finally stopped for water."),
    5783: ("formerly", "The building was formerly a textile mill before it was converted into loft apartments."),
    5791: ("harmful", "Prolonged exposure to ultraviolet radiation is harmful to both skin and eyes."),
    5809: ("intensifier", "In linguistics, the word 'very' functions as a common intensifier that strengthens adjectives."),
    5813: ("investigation", "The internal investigation uncovered a pattern of fraudulent billing spanning three years."),
    5855: ("publicly", "The senator publicly apologized for the misleading statements made during the hearing."),
    5856: ("punishment", "The court determined that the punishment should fit the severity of the offense."),
    5885: ("strongly", "Doctors strongly advise against skipping prescribed doses of the antibiotic."),
    5890: ("tenderness", "There was a tenderness in her voice that made even the simplest words feel profound."),
    5919: ("verifiable", "Every claim in the report must be supported by verifiable evidence from independent sources."),
    5939: ("finitely", "The set of prime factors of any integer can be expressed finitely using unique decomposition."),
    5951: ("unionize", "Workers at the warehouse voted to unionize after months of negotiations with management."),
    6012: ("unalike", "Though raised in the same household, the two siblings were remarkably unalike in temperament."),
    6053: ("proscribe", "International treaties proscribe the use of chemical weapons under any circumstances."),
    6080: ("protuberance", "A bony protuberance on the skull served as the anchor point for powerful jaw muscles."),
    6100: ("exactness", "The exactness of the measurements allowed engineers to assemble the satellite with microscopic tolerances."),
    6234: ("diatomic", "Oxygen exists naturally as a diatomic molecule consisting of two bonded atoms."),
    6241: ("nonmetallic", "Carbon and sulfur are nonmetallic elements that play essential roles in organic chemistry."),
    6250: ("wingless", "The island's wingless beetle evolved in isolation over millions of years."),
    6341: ("inventiveness", "The inventiveness of Renaissance engineers laid the groundwork for modern mechanical design."),
    6357: ("equidistant", "The three monitoring stations are equidistant from the volcano's central crater."),
    6391: ("unvarying", "The metronome's unvarying tempo kept the orchestra perfectly synchronized throughout the movement."),
    6450: ("autocracy", "Under the autocracy, political dissent was suppressed and the press was tightly controlled."),
}


def mint(word_id: int, lemma: str, sentence: str):
    low = sentence.lower()
    idx = low.find(lemma.lower())
    if idx == -1:
        print(f"  SKIP {word_id} ({lemma}): lemma not found in sentence", file=sys.stderr)
        return False
    hl_start = idx
    hl_end = idx + len(lemma)
    body = json.dumps({
        "word_id": word_id,
        "text": sentence,
        "hl_start": hl_start,
        "hl_end": hl_end,
        "source": SOURCE,
    }).encode()
    req = urllib.request.Request(
        f"{API}/api/candidates/example",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req) as resp:
            result = json.loads(resp.read())
            print(f"  OK {word_id} ({lemma}): cand_id={result.get('cand_id', '?')}")
            return True
    except urllib.error.HTTPError as e:
        err = e.read().decode()
        print(f"  ERR {word_id} ({lemma}): {e.code} {err}", file=sys.stderr)
        return False


def main():
    ok = 0
    fail = 0
    for wid, (lemma, sentence) in sorted(WORDS.items()):
        if mint(wid, lemma, sentence):
            ok += 1
        else:
            fail += 1
    print(f"\nDone: {ok} minted, {fail} failed")


if __name__ == "__main__":
    main()
