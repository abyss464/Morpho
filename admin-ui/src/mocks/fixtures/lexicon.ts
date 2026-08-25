/**
 * Seed lexicon for the MSW fixture database.
 *
 * Real exam vocabulary with plausible English-only definitions written from the
 * base + target vocabulary, exactly as the product requires. `stage` drives how
 * complete the generated assets are, which is what gives the console a realistic
 * spread of readiness and blockers.
 */

import type {
  DefinitionSource,
  EtymologySource,
  ExampleSource,
  Pos,
  WordRole,
} from '../../api/types';

/** How far this word has travelled through the reconciliation pipeline. */
export type SeedStage =
  | 'ready' // every gate passed
  | 'distractor_gap' // core_ready, but a bound distractor is not
  | 'pending_approval' // selections exist, a human has not signed off
  | 'oos' // a selected definition still contains an out-of-scope token
  | 'no_image' // image lane exhausted / not fetched yet
  | 'tts_failed' // a synthesis job died
  | 'thin' // fetched, but not enough candidates to fill the slots
  | 'fresh' // just promoted, effectively zero assets
  | 'base'; // base word: never gets assets

export interface SeedSense {
  pos: Pos;
  /** First entry becomes the auto-selected candidate unless a later one scores higher. */
  candidates: Array<{ text: string; source: DefinitionSource; score?: number }>;
}

export interface SeedExample {
  text: string;
  /** Substring of `text` to highlight; offsets are computed by the generator. */
  highlight: string;
  source: ExampleSource;
  score?: number;
}

export interface SeedWord {
  lemma: string;
  role: WordRole;
  phonetic?: string;
  rank?: number;
  etymology?: string;
  etymology_source?: EtymologySource;
  senses?: SeedSense[];
  examples?: SeedExample[];
  image_query?: string;
  stage: SeedStage;
  /** Distractor lemmas, bound once and never recomputed (product rule). */
  distractors?: [string, string, string];
}

export const SEED_WORDS: SeedWord[] = [
  {
    lemma: 'abandon',
    role: 'target',
    phonetic: '/əˈbændən/',
    rank: 412,
    etymology: 'From Old French "a bandon" — left to anyone\'s power, given up freely.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'empty abandoned house',
    distractors: ['abolish', 'accumulate', 'adopt'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to leave a person, thing or place with no plan to return',
            source: 'freedict',
            score: 0.91,
          },
          {
            text: 'forsake, leave behind, or withdraw support from',
            source: 'wordnet',
            score: 0.74,
          },
        ],
      },
      {
        pos: 'noun',
        candidates: [
          {
            text: 'a free and open manner that pays no regard to rules',
            source: 'freedict',
            score: 0.68,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'The crew had to abandon the ship within minutes of the alarm.',
        highlight: 'abandon',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'She refused to abandon the project after a single poor result.',
        highlight: 'abandon',
        source: 'exam_corpus',
        score: 0.81,
      },
      {
        text: 'Do not abandon a plan simply because it is difficult.',
        highlight: 'abandon',
        source: 'llm',
        score: 0.63,
      },
    ],
  },
  {
    lemma: 'abolish',
    role: 'target',
    phonetic: '/əˈbɒlɪʃ/',
    rank: 1284,
    etymology: 'From Latin "abolere" — to destroy, to do away with.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'law book gavel',
    distractors: ['abandon', 'condemn', 'compel'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to cease a law, system or practice by official action',
            source: 'freedict',
            score: 0.93,
          },
          { text: 'do away with, put an end to formally', source: 'wordnet', score: 0.7 },
        ],
      },
    ],
    examples: [
      {
        text: 'The council voted to abolish the entry fee for local residents.',
        highlight: 'abolish',
        source: 'exam_corpus',
        score: 0.86,
      },
      {
        text: 'Many states moved to abolish the old tax within a single year.',
        highlight: 'abolish',
        source: 'exam_corpus',
        score: 0.79,
      },
    ],
  },
  {
    lemma: 'accumulate',
    role: 'target',
    phonetic: '/əˈkjuːmjəleɪt/',
    rank: 903,
    etymology: 'From Latin "accumulare" — to heap up, from "cumulus" (heap).',
    etymology_source: 'wiktionary',
    stage: 'pending_approval',
    image_query: 'stack of coins growing',
    distractors: ['alleviate', 'ascertain', 'devise'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to gather or collect a growing amount over time',
            source: 'freedict',
            score: 0.9,
          },
          {
            text: 'to grow larger in number or quantity little by little',
            source: 'wordnet',
            score: 0.82,
          },
          { text: 'to build up a store of something step by step', source: 'manual', score: 0.87 },
        ],
      },
    ],
    examples: [
      {
        text: 'Dust will accumulate quickly on shelves that nobody cleans.',
        highlight: 'accumulate',
        source: 'exam_corpus',
        score: 0.84,
      },
      {
        text: 'Small savings accumulate into a useful sum over many years.',
        highlight: 'accumulate',
        source: 'llm',
        score: 0.71,
      },
    ],
  },
  {
    lemma: 'adapt',
    role: 'target',
    phonetic: '/əˈdæpt/',
    rank: 356,
    etymology: 'From Latin "adaptare" — to fit to, from "ad-" (to) + "aptare" (to fit).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'chameleon on branch',
    distractors: ['adopt', 'adept', 'devise'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to change something so that it fits a new use or new conditions',
            source: 'freedict',
            score: 0.94,
          },
          { text: 'make fit for a new purpose by changing it', source: 'wordnet', score: 0.76 },
        ],
      },
    ],
    examples: [
      {
        text: 'Students who adapt quickly to new methods tend to learn faster.',
        highlight: 'adapt',
        source: 'exam_corpus',
        score: 0.89,
      },
      {
        text: 'The team had to adapt the design for a much smaller room.',
        highlight: 'adapt',
        source: 'exam_corpus',
        score: 0.83,
      },
      {
        text: 'Plants adapt to dry ground by growing far deeper roots.',
        highlight: 'adapt',
        source: 'llm',
        score: 0.66,
      },
    ],
  },
  {
    lemma: 'adept',
    role: 'target',
    phonetic: '/əˈdept/',
    rank: 2140,
    etymology: 'From Latin "adeptus" — one who has attained, past participle of "adipisci".',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'skilled craftsman hands',
    distractors: ['adapt', 'adopt', 'diligent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'very skilled at something after a great deal of diligent practice',
            source: 'freedict',
            score: 0.92,
          },
          { text: 'having or showing high skill', source: 'wordnet', score: 0.73 },
        ],
      },
    ],
    examples: [
      {
        text: 'She is adept at reading long reports and finding the key point.',
        highlight: 'adept',
        source: 'exam_corpus',
        score: 0.85,
      },
      {
        text: 'A adept guide can lead a group through rough ground safely.',
        highlight: 'adept',
        source: 'llm',
        score: 0.58,
      },
    ],
  },
  {
    lemma: 'adopt',
    role: 'target',
    phonetic: '/əˈdɒpt/',
    rank: 288,
    etymology: 'From Latin "adoptare" — to choose for oneself.',
    etymology_source: 'wiktionary',
    stage: 'distractor_gap',
    image_query: 'hands holding new plan document',
    distractors: ['adapt', 'adept', 'endorse'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to take up and start using a method, idea or plan',
            source: 'freedict',
            score: 0.91,
          },
          {
            text: 'to take a child into your family as your own by law',
            source: 'freedict',
            score: 0.86,
          },
          { text: 'choose and follow a course of action', source: 'wordnet', score: 0.72 },
        ],
      },
    ],
    examples: [
      {
        text: 'The school will adopt a new marking system in the coming term.',
        highlight: 'adopt',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'Firms that adopt clear rules early avoid trouble later.',
        highlight: 'adopt',
        source: 'llm',
        score: 0.69,
      },
    ],
  },
  {
    lemma: 'advocate',
    role: 'target',
    phonetic: '/ˈædvəkeɪt/',
    rank: 977,
    etymology: 'From Latin "advocatus" — one called to aid, from "advocare" (to call to).',
    etymology_source: 'wiktionary',
    stage: 'pending_approval',
    image_query: 'person speaking at public meeting',
    distractors: ['endorse', 'condemn', 'compel'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to speak or write in public support of an idea or plan',
            source: 'freedict',
            score: 0.9,
          },
          { text: 'to argue openly in favour of something', source: 'wordnet', score: 0.78 },
        ],
      },
      {
        pos: 'noun',
        candidates: [
          {
            text: 'a person who publicly supports a cause or another person',
            source: 'freedict',
            score: 0.88,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'Doctors advocate regular walking for people who sit all day.',
        highlight: 'advocate',
        source: 'exam_corpus',
        score: 0.84,
      },
      {
        text: 'He became a strong advocate for cleaner air in the city.',
        highlight: 'advocate',
        source: 'exam_corpus',
        score: 0.8,
      },
    ],
  },
  {
    lemma: 'alleviate',
    role: 'target',
    phonetic: '/əˈliːvieɪt/',
    rank: 1866,
    etymology: 'From Late Latin "alleviare" — to lighten, from "levis" (light).',
    etymology_source: 'wiktionary',
    stage: 'no_image',
    image_query: 'nurse comforting patient',
    distractors: ['mitigate', 'exacerbate', 'hinder'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          { text: 'to mitigate pain, suffering or a problem', source: 'freedict', score: 0.93 },
          { text: 'provide relief for; make easier to bear', source: 'wordnet', score: 0.75 },
        ],
      },
    ],
    examples: [
      {
        text: 'A short rest can alleviate the worst of the pain.',
        highlight: 'alleviate',
        source: 'exam_corpus',
        score: 0.82,
      },
      {
        text: 'The new road did little to alleviate traffic in the old town.',
        highlight: 'alleviate',
        source: 'exam_corpus',
        score: 0.86,
      },
    ],
  },
  {
    lemma: 'ambiguous',
    role: 'target',
    phonetic: '/æmˈbɪɡjuəs/',
    rank: 1421,
    etymology: 'From Latin "ambiguus" — going both ways, doubtful.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'road sign pointing two ways',
    distractors: ['lucid', 'coherent', 'plausible'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'open to more than one reading, so the meaning is not clear',
            source: 'freedict',
            score: 0.94,
          },
          { text: 'having more than one possible meaning', source: 'wordnet', score: 0.81 },
        ],
      },
    ],
    examples: [
      {
        text: 'The wording of the rule was ambiguous and led to two readings.',
        highlight: 'ambiguous',
        source: 'exam_corpus',
        score: 0.9,
      },
      {
        text: 'An ambiguous answer helps nobody in a serious meeting.',
        highlight: 'ambiguous',
        source: 'llm',
        score: 0.7,
      },
    ],
  },
  {
    lemma: 'arbitrary',
    role: 'target',
    phonetic: '/ˈɑːbɪtrəri/',
    rank: 1188,
    etymology: 'From Latin "arbitrarius" — depending on the will of a judge.',
    etymology_source: 'wiktionary',
    stage: 'oos',
    image_query: 'dice roll on table',
    distractors: ['impartial', 'prudent', 'pragmatic'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'chosen by personal whim rather than by reason or rule',
            source: 'freedict',
            score: 0.89,
          },
          {
            text: 'based on random choice rather than any clear plan',
            source: 'wordnet',
            score: 0.8,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'The order of names on the list looked entirely arbitrary.',
        highlight: 'arbitrary',
        source: 'exam_corpus',
        score: 0.85,
      },
      {
        text: 'An arbitrary limit on study hours would help nobody.',
        highlight: 'arbitrary',
        source: 'llm',
        score: 0.64,
      },
    ],
  },
  {
    lemma: 'ascertain',
    role: 'target',
    phonetic: '/ˌæsəˈteɪn/',
    rank: 1732,
    etymology: 'From Old French "acertener" — to make certain.',
    etymology_source: 'wiktionary',
    stage: 'tts_failed',
    image_query: 'magnifying glass over documents',
    distractors: ['discern', 'scrutinize', 'devise'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to find out something with certainty by checking it',
            source: 'freedict',
            score: 0.9,
          },
          { text: 'establish after careful study', source: 'wordnet', score: 0.76 },
        ],
      },
    ],
    examples: [
      {
        text: 'Police were unable to ascertain how the fire had started.',
        highlight: 'ascertain',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'It is hard to ascertain the true cost before the work begins.',
        highlight: 'ascertain',
        source: 'exam_corpus',
        score: 0.79,
      },
    ],
  },
  {
    lemma: 'benevolent',
    role: 'target',
    phonetic: '/bəˈnevələnt/',
    rank: 2288,
    etymology: 'From Latin "bene" (well) + "volens" (wishing).',
    etymology_source: 'wiktionary',
    stage: 'oos',
    image_query: 'person giving food to stranger',
    distractors: ['hostile', 'jovial', 'courteous'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'well meaning and kindly toward other people', source: 'freedict', score: 0.92 },
          {
            text: 'showing an altruistic wish to help those in need',
            source: 'wordnet',
            score: 0.86,
          },
          {
            text: 'wishing others well and acting in a kindly way',
            source: 'llm_rewrite',
            score: 0.84,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'A benevolent reader will forgive a few small mistakes.',
        highlight: 'benevolent',
        source: 'exam_corpus',
        score: 0.83,
      },
      {
        text: 'The fund was set up by a benevolent former student.',
        highlight: 'benevolent',
        source: 'exam_corpus',
        score: 0.87,
      },
    ],
  },
  {
    lemma: 'candid',
    role: 'target',
    phonetic: '/ˈkændɪd/',
    rank: 2011,
    etymology: 'From Latin "candidus" — white, pure, hence open and honest.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'two people talking honestly',
    distractors: ['sincere', 'eloquent', 'impartial'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'open and honest in speech, even when the truth is not welcome',
            source: 'freedict',
            score: 0.91,
          },
          { text: 'free from pretence or reserve', source: 'wordnet', score: 0.74 },
        ],
      },
    ],
    examples: [
      {
        text: 'She gave a candid account of what had gone wrong that week.',
        highlight: 'candid',
        source: 'exam_corpus',
        score: 0.86,
      },
      {
        text: 'Be candid with your tutor about the parts you find hard.',
        highlight: 'candid',
        source: 'llm',
        score: 0.68,
      },
    ],
  },
  {
    lemma: 'cease',
    role: 'target',
    phonetic: '/siːs/',
    rank: 742,
    etymology: 'From Latin "cessare" — to stop, to hold back.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'stop sign on empty road',
    distractors: ['curb', 'hinder', 'compel'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to stop doing something, or to come to an end',
            source: 'freedict',
            score: 0.93,
          },
          { text: 'put an end to a state or an activity', source: 'wordnet', score: 0.77 },
        ],
      },
    ],
    examples: [
      {
        text: 'The rain did not cease until late in the evening.',
        highlight: 'cease',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'Work on the old bridge will cease at the end of the month.',
        highlight: 'cease',
        source: 'exam_corpus',
        score: 0.82,
      },
    ],
  },
  {
    lemma: 'coherent',
    role: 'target',
    phonetic: '/kəʊˈhɪərənt/',
    rank: 1345,
    etymology: 'From Latin "cohaerere" — to stick together.',
    etymology_source: 'wiktionary',
    stage: 'pending_approval',
    image_query: 'neatly linked chain',
    distractors: ['lucid', 'ambiguous', 'plausible'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'holding together in a clear and lucid way', source: 'freedict', score: 0.92 },
          {
            text: 'marked by an orderly and consistent relation of parts',
            source: 'wordnet',
            score: 0.79,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'A coherent answer moves from one point to the next without gaps.',
        highlight: 'coherent',
        source: 'exam_corpus',
        score: 0.89,
      },
      {
        text: 'He was too tired to give a coherent account of the accident.',
        highlight: 'coherent',
        source: 'exam_corpus',
        score: 0.84,
      },
    ],
  },
  {
    lemma: 'compel',
    role: 'target',
    phonetic: '/kəmˈpel/',
    rank: 1052,
    etymology: 'From Latin "compellere" — to drive together, to force.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'hand pushing heavy door',
    distractors: ['condemn', 'curb', 'cease'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to force a person to do something by pressure or by rule',
            source: 'freedict',
            score: 0.91,
          },
          { text: 'necessitate or exact by force', source: 'wordnet', score: 0.72 },
        ],
      },
    ],
    examples: [
      {
        text: 'Bad weather may compel the team to delay the whole trip.',
        highlight: 'compel',
        source: 'exam_corpus',
        score: 0.85,
      },
      {
        text: 'No rule can compel a reader to enjoy a dull book.',
        highlight: 'compel',
        source: 'llm',
        score: 0.67,
      },
    ],
  },
  {
    lemma: 'condemn',
    role: 'target',
    phonetic: '/kənˈdem/',
    rank: 1109,
    etymology: 'From Latin "condemnare" — to sentence, to blame fully.',
    etymology_source: 'wiktionary',
    stage: 'thin',
    image_query: 'crowd protesting with signs',
    distractors: ['endorse', 'advocate', 'compel'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to say strongly in public that something is very wrong',
            source: 'freedict',
            score: 0.9,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'Leaders were quick to condemn the attack on the school.',
        highlight: 'condemn',
        source: 'exam_corpus',
        score: 0.86,
      },
    ],
  },
  {
    lemma: 'constitute',
    role: 'target',
    phonetic: '/ˈkɒnstɪtjuːt/',
    rank: 826,
    etymology: 'From Latin "constituere" — to set up, to establish.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'puzzle pieces forming a whole',
    distractors: ['comprise', 'devise', 'implement'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          { text: 'to be the parts that comprise a whole', source: 'freedict', score: 0.9 },
          { text: 'form or compose', source: 'wordnet', score: 0.71 },
        ],
      },
    ],
    examples: [
      {
        text: 'Women constitute over half of the students on this course.',
        highlight: 'constitute',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'Three short talks constitute the whole of the morning session.',
        highlight: 'constitute',
        source: 'exam_corpus',
        score: 0.78,
      },
    ],
  },
  {
    lemma: 'curb',
    role: 'target',
    phonetic: '/kɜːb/',
    rank: 1497,
    etymology: 'From Old French "courbe" — a curved strap used to check a horse.',
    etymology_source: 'wiktionary',
    stage: 'no_image',
    image_query: 'hand holding back a rope',
    distractors: ['hinder', 'cease', 'mitigate'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to hinder or limit something that is growing too fast',
            source: 'freedict',
            score: 0.9,
          },
          { text: 'keep within bounds', source: 'wordnet', score: 0.73 },
        ],
      },
    ],
    examples: [
      {
        text: 'The city raised parking fees to curb the number of cars.',
        highlight: 'curb',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'She has learned to curb her habit of speaking too soon.',
        highlight: 'curb',
        source: 'llm',
        score: 0.65,
      },
    ],
  },
  {
    lemma: 'deteriorate',
    role: 'target',
    phonetic: '/dɪˈtɪəriəreɪt/',
    rank: 1613,
    etymology: 'From Latin "deteriorare" — to make worse, from "deterior" (worse).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'rusted metal wall peeling',
    distractors: ['exacerbate', 'undermine', 'hinder'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to become worse in condition or quality over time',
            source: 'freedict',
            score: 0.93,
          },
          { text: 'grow worse', source: 'wordnet', score: 0.7 },
        ],
      },
    ],
    examples: [
      {
        text: 'His health began to deteriorate soon after the long journey.',
        highlight: 'deteriorate',
        source: 'exam_corpus',
        score: 0.89,
      },
      {
        text: 'Old paper will deteriorate quickly in a warm damp room.',
        highlight: 'deteriorate',
        source: 'exam_corpus',
        score: 0.81,
      },
    ],
  },
  {
    lemma: 'devise',
    role: 'target',
    phonetic: '/dɪˈvaɪz/',
    rank: 1298,
    etymology: 'From Old French "deviser" — to divide, to plan out.',
    etymology_source: 'wiktionary',
    stage: 'pending_approval',
    image_query: 'engineer sketching a plan',
    distractors: ['implement', 'ascertain', 'adapt'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to plan or invent a method by careful thought',
            source: 'freedict',
            score: 0.91,
          },
          { text: 'come up with after mental effort', source: 'wordnet', score: 0.74 },
        ],
      },
    ],
    examples: [
      {
        text: 'They had to devise a cheaper way to test each sample.',
        highlight: 'devise',
        source: 'exam_corpus',
        score: 0.86,
      },
      {
        text: 'Teachers devise short tasks that fit into a single lesson.',
        highlight: 'devise',
        source: 'llm',
        score: 0.68,
      },
    ],
  },
  {
    lemma: 'diligent',
    role: 'target',
    phonetic: '/ˈdɪlɪdʒənt/',
    rank: 1774,
    etymology: 'From Latin "diligens" — attentive, careful, from "diligere" (to value).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'student studying at desk late',
    distractors: ['meticulous', 'adept', 'prudent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'working hard with steady and thorough care', source: 'freedict', score: 0.92 },
          { text: 'characterized by care and steady effort', source: 'wordnet', score: 0.78 },
        ],
      },
    ],
    examples: [
      {
        text: 'A diligent worker checks the result before handing it in.',
        highlight: 'diligent',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'Years of diligent practice made the piece sound easy.',
        highlight: 'diligent',
        source: 'exam_corpus',
        score: 0.83,
      },
    ],
  },
  {
    lemma: 'discern',
    role: 'target',
    phonetic: '/dɪˈsɜːn/',
    rank: 1655,
    etymology: 'From Latin "discernere" — to separate, to distinguish.',
    etymology_source: 'wiktionary',
    stage: 'distractor_gap',
    image_query: 'eye looking through fog',
    distractors: ['scrutinize', 'ascertain', 'lucid'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to see or understand something that is not obvious',
            source: 'freedict',
            score: 0.9,
          },
          { text: 'detect with the senses', source: 'wordnet', score: 0.72 },
        ],
      },
    ],
    examples: [
      {
        text: 'It was hard to discern any pattern in the early results.',
        highlight: 'discern',
        source: 'exam_corpus',
        score: 0.85,
      },
      {
        text: 'A careful reader can discern the writer real view.',
        highlight: 'discern',
        source: 'llm',
        score: 0.6,
      },
    ],
  },
  {
    lemma: 'eloquent',
    role: 'target',
    phonetic: '/ˈeləkwənt/',
    rank: 2064,
    etymology: 'From Latin "eloquens" — speaking out, from "eloqui" (to speak out).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'speaker addressing an audience',
    distractors: ['lucid', 'candid', 'coherent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'speaking or writing with easy flow and real force',
            source: 'freedict',
            score: 0.91,
          },
          { text: 'expressing yourself readily and effectively', source: 'wordnet', score: 0.76 },
        ],
      },
    ],
    examples: [
      {
        text: 'Her eloquent closing speech won over most of the room.',
        highlight: 'eloquent',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'The letter was short but eloquent about what had been lost.',
        highlight: 'eloquent',
        source: 'exam_corpus',
        score: 0.8,
      },
    ],
  },
  {
    lemma: 'endorse',
    role: 'target',
    phonetic: '/ɪnˈdɔːs/',
    rank: 1226,
    etymology: 'From Medieval Latin "indorsare" — to write on the back of a document.',
    etymology_source: 'wiktionary',
    stage: 'tts_failed',
    image_query: 'signing a document with pen',
    distractors: ['advocate', 'condemn', 'adopt'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to give public approval and support to a person or plan',
            source: 'freedict',
            score: 0.92,
          },
          { text: 'be behind; approve of', source: 'wordnet', score: 0.71 },
        ],
      },
    ],
    examples: [
      {
        text: 'Several large firms agreed to endorse the new safety rule.',
        highlight: 'endorse',
        source: 'exam_corpus',
        score: 0.86,
      },
      {
        text: 'The board would not endorse a plan it had not seen.',
        highlight: 'endorse',
        source: 'exam_corpus',
        score: 0.79,
      },
    ],
  },
  {
    lemma: 'exacerbate',
    role: 'target',
    phonetic: '/ɪɡˈzæsəbeɪt/',
    rank: 2317,
    etymology: 'From Latin "exacerbare" — to make bitter, from "acerbus" (harsh).',
    etymology_source: 'wiktionary',
    stage: 'oos',
    image_query: 'small crack spreading in glass',
    distractors: ['alleviate', 'mitigate', 'deteriorate'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          { text: 'to make a bad situation or illness worse', source: 'freedict', score: 0.93 },
          {
            text: 'to aggravate an already unpleasant state of affairs',
            source: 'wordnet',
            score: 0.85,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'Cutting the budget now would exacerbate an already hard year.',
        highlight: 'exacerbate',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'Loud noise can exacerbate a headache within minutes.',
        highlight: 'exacerbate',
        source: 'llm',
        score: 0.7,
      },
    ],
  },
  {
    lemma: 'facilitate',
    role: 'target',
    phonetic: '/fəˈsɪlɪteɪt/',
    rank: 1043,
    etymology: 'From Latin "facilis" — easy to do.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'open gate on a clear path',
    distractors: ['hinder', 'implement', 'devise'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          { text: 'to make an action or process easier', source: 'freedict', score: 0.93 },
          { text: 'make easier or less difficult', source: 'wordnet', score: 0.75 },
        ],
      },
    ],
    examples: [
      {
        text: 'A shared calendar will facilitate planning across the two teams.',
        highlight: 'facilitate',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'Clear labels facilitate a quick search through the boxes.',
        highlight: 'facilitate',
        source: 'llm',
        score: 0.69,
      },
    ],
  },
  {
    lemma: 'frugal',
    role: 'target',
    phonetic: '/ˈfruːɡl/',
    rank: 2455,
    etymology: 'From Latin "frugalis" — useful, thrifty, from "frux" (fruit, produce).',
    etymology_source: 'wiktionary',
    stage: 'thin',
    image_query: 'simple meal on a plain table',
    distractors: ['prudent', 'ample', 'diligent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'careful with money and goods, spending very little',
            source: 'freedict',
            score: 0.91,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'A frugal student can live well on very little each month.',
        highlight: 'frugal',
        source: 'llm',
        score: 0.72,
      },
    ],
  },
  {
    lemma: 'gregarious',
    role: 'target',
    phonetic: '/ɡrɪˈɡeəriəs/',
    rank: 2703,
    etymology: 'From Latin "gregarius" — belonging to a flock, from "grex" (herd).',
    etymology_source: 'wiktionary',
    stage: 'no_image',
    image_query: 'group of friends laughing together',
    distractors: ['jovial', 'courteous', 'hostile'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'fond of company and jovial in a group', source: 'freedict', score: 0.9 },
          {
            text: 'temperamentally seeking and enjoying the company of others',
            source: 'wordnet',
            score: 0.77,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'A gregarious child will settle into a new class quickly.',
        highlight: 'gregarious',
        source: 'exam_corpus',
        score: 0.84,
      },
      {
        text: 'These birds are gregarious and rarely feed alone.',
        highlight: 'gregarious',
        source: 'exam_corpus',
        score: 0.86,
      },
    ],
  },
  {
    lemma: 'hinder',
    role: 'target',
    phonetic: '/ˈhɪndə/',
    rank: 1364,
    etymology: 'From Old English "hindrian" — to keep back.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'fallen tree blocking a path',
    distractors: ['facilitate', 'curb', 'undermine'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          { text: 'to slow or block the progress of something', source: 'freedict', score: 0.92 },
          { text: 'be a hindrance or obstacle to', source: 'wordnet', score: 0.73 },
        ],
      },
    ],
    examples: [
      {
        text: 'Heavy snow will hinder any attempt to reach the top today.',
        highlight: 'hinder',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'Poor lighting can hinder careful reading for hours on end.',
        highlight: 'hinder',
        source: 'llm',
        score: 0.66,
      },
    ],
  },
  {
    lemma: 'hostile',
    role: 'target',
    phonetic: '/ˈhɒstaɪl/',
    rank: 1017,
    etymology: 'From Latin "hostilis" — of an enemy, from "hostis" (stranger, enemy).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'storm over barren land',
    distractors: ['benevolent', 'courteous', 'jovial'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'showing strong dislike and a readiness to fight',
            source: 'freedict',
            score: 0.91,
          },
          { text: 'not friendly; very hard to live or work in', source: 'freedict', score: 0.85 },
        ],
      },
    ],
    examples: [
      {
        text: 'The crowd turned hostile once the result was read out.',
        highlight: 'hostile',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'Few plants survive in so hostile a climate.',
        highlight: 'hostile',
        source: 'exam_corpus',
        score: 0.82,
      },
    ],
  },
  {
    lemma: 'impartial',
    role: 'target',
    phonetic: '/ɪmˈpɑːʃl/',
    rank: 1902,
    etymology: 'From "in-" (not) + "partial" — not taking a part or side.',
    etymology_source: 'morfessor',
    stage: 'pending_approval',
    image_query: 'balanced scales of justice',
    distractors: ['arbitrary', 'candid', 'prudent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'not favouring one side over the other', source: 'freedict', score: 0.93 },
          { text: 'free from undue bias or preconceived opinions', source: 'wordnet', score: 0.79 },
        ],
      },
    ],
    examples: [
      {
        text: 'An impartial judge listens to both sides before deciding.',
        highlight: 'impartial',
        source: 'exam_corpus',
        score: 0.89,
      },
      {
        text: 'The report was praised for its impartial handling of the facts.',
        highlight: 'impartial',
        source: 'exam_corpus',
        score: 0.81,
      },
    ],
  },
  {
    lemma: 'implement',
    role: 'target',
    phonetic: '/ˈɪmplɪment/',
    rank: 634,
    etymology: 'From Late Latin "implementum" — a filling up, a means of carrying out.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'workers building from a blueprint',
    distractors: ['devise', 'facilitate', 'adopt'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to put a plan or decision into actual practice',
            source: 'freedict',
            score: 0.94,
          },
          { text: 'apply in a manner consistent with its purpose', source: 'wordnet', score: 0.72 },
        ],
      },
      {
        pos: 'noun',
        candidates: [
          { text: 'a tool used for a particular task', source: 'freedict', score: 0.85 },
        ],
      },
    ],
    examples: [
      {
        text: 'The school will implement the new timetable after the holiday.',
        highlight: 'implement',
        source: 'exam_corpus',
        score: 0.9,
      },
      {
        text: 'A good idea is worthless until someone can implement it.',
        highlight: 'implement',
        source: 'llm',
        score: 0.7,
      },
    ],
  },
  {
    lemma: 'jovial',
    role: 'target',
    phonetic: '/ˈdʒəʊviəl/',
    rank: 2891,
    etymology: 'From Latin "jovialis" — of Jupiter, whose influence was thought merry.',
    etymology_source: 'wiktionary',
    stage: 'thin',
    image_query: 'cheerful man laughing',
    distractors: ['gregarious', 'hostile', 'courteous'],
    senses: [
      {
        pos: 'adj',
        candidates: [{ text: 'cheerful and friendly in manner', source: 'freedict', score: 0.88 }],
      },
    ],
    examples: [
      {
        text: 'Our jovial guide kept the whole group in good spirits.',
        highlight: 'jovial',
        source: 'exam_corpus',
        score: 0.83,
      },
    ],
  },
  {
    lemma: 'lucid',
    role: 'target',
    phonetic: '/ˈluːsɪd/',
    rank: 2402,
    etymology: 'From Latin "lucidus" — bright, clear, from "lux" (light).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'clear still water over stones',
    distractors: ['coherent', 'ambiguous', 'eloquent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'clear and easy to follow', source: 'freedict', score: 0.92 },
          { text: 'transparently clear; easily understandable', source: 'wordnet', score: 0.8 },
        ],
      },
    ],
    examples: [
      {
        text: 'He gave a lucid account of a very complex court case.',
        highlight: 'lucid',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'The chapter is short, lucid, and free of empty words.',
        highlight: 'lucid',
        source: 'llm',
        score: 0.69,
      },
    ],
  },
  {
    lemma: 'meticulous',
    role: 'target',
    phonetic: '/məˈtɪkjələs/',
    rank: 2233,
    etymology: 'From Latin "meticulosus" — fearful, later careful about small things.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'watchmaker working with tweezers',
    distractors: ['diligent', 'thorough', 'prudent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'showing thorough care about small details', source: 'freedict', score: 0.94 },
          {
            text: 'marked by extreme care in treatment of details',
            source: 'wordnet',
            score: 0.81,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'Her meticulous notes made the whole review far quicker.',
        highlight: 'meticulous',
        source: 'exam_corpus',
        score: 0.89,
      },
      {
        text: 'The repair called for meticulous work over several days.',
        highlight: 'meticulous',
        source: 'exam_corpus',
        score: 0.84,
      },
    ],
  },
  {
    lemma: 'mitigate',
    role: 'target',
    phonetic: '/ˈmɪtɪɡeɪt/',
    rank: 1958,
    etymology: 'From Latin "mitigare" — to soften, from "mitis" (mild).',
    etymology_source: 'wiktionary',
    stage: 'distractor_gap',
    image_query: 'sea wall holding back waves',
    distractors: ['alleviate', 'exacerbate', 'curb'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          { text: 'to make the harm or force of something less', source: 'freedict', score: 0.92 },
          { text: 'lessen or to try to lessen the seriousness of', source: 'wordnet', score: 0.76 },
        ],
      },
    ],
    examples: [
      {
        text: 'Planting trees can mitigate the worst of the summer heat.',
        highlight: 'mitigate',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'Nothing said in court could mitigate so serious a charge.',
        highlight: 'mitigate',
        source: 'exam_corpus',
        score: 0.8,
      },
    ],
  },
  {
    lemma: 'mundane',
    role: 'target',
    phonetic: '/mʌnˈdeɪn/',
    rank: 2588,
    etymology: 'From Latin "mundanus" — belonging to the world.',
    etymology_source: 'wiktionary',
    stage: 'no_image',
    image_query: 'grey office desk routine',
    distractors: ['notorious', 'obsolete', 'ample'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'ordinary and dull, lacking interest', source: 'freedict', score: 0.9 },
          { text: 'found in the ordinary course of events', source: 'wordnet', score: 0.74 },
        ],
      },
    ],
    examples: [
      {
        text: 'Most of the work is mundane, but it has to be done well.',
        highlight: 'mundane',
        source: 'exam_corpus',
        score: 0.85,
      },
      {
        text: 'She found even mundane tasks worth doing with care.',
        highlight: 'mundane',
        source: 'llm',
        score: 0.67,
      },
    ],
  },
  {
    lemma: 'notorious',
    role: 'target',
    phonetic: '/nəʊˈtɔːriəs/',
    rank: 1683,
    etymology: 'From Medieval Latin "notorius" — well known, from "noscere" (to know).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'newspaper headline scandal',
    distractors: ['mundane', 'hostile', 'obsolete'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'widely known for hostile or shameful acts', source: 'freedict', score: 0.93 },
          { text: 'known widely and usually unfavourably', source: 'wordnet', score: 0.78 },
        ],
      },
    ],
    examples: [
      {
        text: 'That road is notorious for long delays every winter.',
        highlight: 'notorious',
        source: 'exam_corpus',
        score: 0.89,
      },
      {
        text: 'He was notorious among students for very hard tests.',
        highlight: 'notorious',
        source: 'exam_corpus',
        score: 0.82,
      },
    ],
  },
  {
    lemma: 'obsolete',
    role: 'target',
    phonetic: '/ˈɒbsəliːt/',
    rank: 1541,
    etymology: 'From Latin "obsoletus" — worn out, gone out of use.',
    etymology_source: 'wiktionary',
    stage: 'pending_approval',
    image_query: 'old typewriter covered in dust',
    distractors: ['mundane', 'notorious', 'ample'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'no longer in use because something newer has replaced it',
            source: 'freedict',
            score: 0.93,
          },
          { text: 'no longer in use', source: 'wordnet', score: 0.7 },
        ],
      },
    ],
    examples: [
      {
        text: 'The old file format became obsolete within three years.',
        highlight: 'obsolete',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'Few skills go obsolete as fast as those tied to one tool.',
        highlight: 'obsolete',
        source: 'llm',
        score: 0.68,
      },
    ],
  },
  {
    lemma: 'plausible',
    role: 'target',
    phonetic: '/ˈplɔːzəbl/',
    rank: 1470,
    etymology: 'From Latin "plausibilis" — deserving applause, from "plaudere" (to clap).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'detective examining clues board',
    distractors: ['ambiguous', 'coherent', 'pragmatic'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'seeming reasonable and lucid enough to be true',
            source: 'freedict',
            score: 0.93,
          },
          { text: 'apparently reasonable and valid', source: 'wordnet', score: 0.77 },
        ],
      },
    ],
    examples: [
      {
        text: 'Only one plausible reason remained once the others failed.',
        highlight: 'plausible',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'A plausible story is not the same as a true one.',
        highlight: 'plausible',
        source: 'exam_corpus',
        score: 0.85,
      },
    ],
  },
  {
    lemma: 'pragmatic',
    role: 'target',
    phonetic: '/præɡˈmætɪk/',
    rank: 1391,
    etymology: 'From Greek "pragmatikos" — relating to deeds, from "pragma" (deed).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'mechanic fixing engine practically',
    distractors: ['prudent', 'arbitrary', 'plausible'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'dealing with things in a practical way rather than by theory',
            source: 'freedict',
            score: 0.92,
          },
          { text: 'concerned with practical matters', source: 'wordnet', score: 0.75 },
        ],
      },
    ],
    examples: [
      {
        text: 'A pragmatic answer beats a perfect one that never arrives.',
        highlight: 'pragmatic',
        source: 'exam_corpus',
        score: 0.86,
      },
      {
        text: 'She took a pragmatic view and cut the least useful task.',
        highlight: 'pragmatic',
        source: 'exam_corpus',
        score: 0.83,
      },
    ],
  },
  {
    lemma: 'prudent',
    role: 'target',
    phonetic: '/ˈpruːdnt/',
    rank: 1809,
    etymology: 'From Latin "prudens" — foreseeing, wise, a contraction of "providens".',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'person checking map before journey',
    distractors: ['cautious', 'pragmatic', 'frugal'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'cautious and showing good judgement about what lies ahead',
            source: 'freedict',
            score: 0.93,
          },
          {
            text: 'careful and sensible; marked by sound judgment',
            source: 'wordnet',
            score: 0.79,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'It would be prudent to save part of every payment.',
        highlight: 'prudent',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'A prudent driver slows down long before the corner.',
        highlight: 'prudent',
        source: 'llm',
        score: 0.7,
      },
    ],
  },
  {
    lemma: 'reluctant',
    role: 'target',
    phonetic: '/rɪˈlʌktənt/',
    rank: 895,
    etymology: 'From Latin "reluctans" — struggling against.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'child hesitating at door',
    distractors: ['wary', 'cautious', 'hostile'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'unwilling to act, and slow to agree', source: 'freedict', score: 0.92 },
          { text: 'not eager; disinclined', source: 'wordnet', score: 0.74 },
        ],
      },
    ],
    examples: [
      {
        text: 'He was reluctant to speak until he had read the whole file.',
        highlight: 'reluctant',
        source: 'exam_corpus',
        score: 0.89,
      },
      {
        text: 'Firms remain reluctant to hire while costs keep rising.',
        highlight: 'reluctant',
        source: 'exam_corpus',
        score: 0.84,
      },
    ],
  },
  {
    lemma: 'resilient',
    role: 'target',
    phonetic: '/rɪˈzɪliənt/',
    rank: 1622,
    etymology: 'From Latin "resilire" — to leap back, to rebound.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'young tree bending in wind',
    distractors: ['tenacious', 'steadfast', 'diligent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'able to recover quickly after difficulty or damage',
            source: 'freedict',
            score: 0.94,
          },
          { text: 'recovering readily from adversity', source: 'wordnet', score: 0.78 },
        ],
      },
    ],
    examples: [
      {
        text: 'Small firms proved more resilient than anyone expected.',
        highlight: 'resilient',
        source: 'exam_corpus',
        score: 0.9,
      },
      {
        text: 'A resilient roof will survive several hard winters.',
        highlight: 'resilient',
        source: 'llm',
        score: 0.68,
      },
    ],
  },
  {
    lemma: 'scrutinize',
    role: 'target',
    phonetic: '/ˈskruːtənaɪz/',
    rank: 2178,
    etymology: 'From Latin "scrutari" — to search through, to examine.',
    etymology_source: 'wiktionary',
    stage: 'tts_failed',
    image_query: 'inspector with magnifier over paper',
    distractors: ['discern', 'ascertain', 'meticulous'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to look at something closely and with great care',
            source: 'freedict',
            score: 0.92,
          },
          { text: 'examine carefully for accuracy', source: 'wordnet', score: 0.76 },
        ],
      },
    ],
    examples: [
      {
        text: 'Editors scrutinize every claim before the piece goes out.',
        highlight: 'scrutinize',
        source: 'exam_corpus',
        score: 0.87,
      },
      {
        text: 'Buyers should scrutinize the small print on any contract.',
        highlight: 'scrutinize',
        source: 'exam_corpus',
        score: 0.81,
      },
    ],
  },
  {
    lemma: 'tenacious',
    role: 'target',
    phonetic: '/təˈneɪʃəs/',
    rank: 2506,
    etymology: 'From Latin "tenax" — holding fast, from "tenere" (to hold).',
    etymology_source: 'wiktionary',
    // A human pinned the WordNet gloss, which later turned out to carry the
    // out-of-scope token "unyielding".
    stage: 'oos',
    image_query: 'climber gripping rock face',
    distractors: ['resilient', 'steadfast', 'diligent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'holding on firmly and refusing to give up', source: 'freedict', score: 0.92 },
          { text: 'stubbornly unyielding', source: 'wordnet', score: 0.73 },
        ],
      },
    ],
    examples: [
      {
        text: 'Her tenacious search finally turned up the missing record.',
        highlight: 'tenacious',
        source: 'exam_corpus',
        score: 0.86,
      },
      {
        text: 'The illness proved tenacious and lasted the whole winter.',
        highlight: 'tenacious',
        source: 'exam_corpus',
        score: 0.8,
      },
    ],
  },
  {
    lemma: 'undermine',
    role: 'target',
    phonetic: '/ˌʌndəˈmaɪn/',
    rank: 1237,
    etymology: 'From "under" + "mine" — to dig beneath a wall to make it fall.',
    etymology_source: 'morfessor',
    stage: 'oos',
    image_query: 'eroded cliff edge below house',
    distractors: ['hinder', 'deteriorate', 'exacerbate'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to weaken something slowly from below or from within',
            source: 'freedict',
            score: 0.92,
          },
          {
            text: 'to erode the base or foundation of something over time',
            source: 'wordnet',
            score: 0.84,
          },
        ],
      },
    ],
    examples: [
      {
        text: 'Constant delays undermine trust in the whole service.',
        highlight: 'undermine',
        source: 'exam_corpus',
        score: 0.88,
      },
      {
        text: 'One careless word can undermine a year of good work.',
        highlight: 'undermine',
        source: 'llm',
        score: 0.71,
      },
    ],
  },
  {
    lemma: 'vindicate',
    role: 'target',
    phonetic: '/ˈvɪndɪkeɪt/',
    rank: 2761,
    etymology: 'From Latin "vindicare" — to claim, to set free, to avenge.',
    etymology_source: 'wiktionary',
    stage: 'fresh',
    image_query: 'person cleared in courtroom',
    distractors: ['endorse', 'advocate', 'condemn'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          {
            text: 'to show that a person or view was right after all',
            source: 'freedict',
            score: 0.9,
          },
        ],
      },
    ],
    examples: [],
  },
  {
    lemma: 'wary',
    role: 'target',
    phonetic: '/ˈweəri/',
    rank: 1573,
    etymology: 'From Old English "wær" — cautious, aware.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'deer alert in forest',
    distractors: ['cautious', 'reluctant', 'prudent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'watchful and cautious about possible danger', source: 'freedict', score: 0.93 },
          { text: 'marked by keen caution and watchful prudence', source: 'wordnet', score: 0.78 },
        ],
      },
    ],
    examples: [
      {
        text: 'Be wary of any offer that sounds far too good.',
        highlight: 'wary',
        source: 'exam_corpus',
        score: 0.89,
      },
      {
        text: 'Older birds stay wary of anything new near the feeder.',
        highlight: 'wary',
        source: 'exam_corpus',
        score: 0.82,
      },
    ],
  },
  {
    lemma: 'comprise',
    role: 'target',
    phonetic: '/kəmˈpraɪz/',
    rank: 1155,
    etymology: 'From Old French "comprendre" — to contain, to include.',
    etymology_source: 'wiktionary',
    stage: 'pending_approval',
    image_query: 'boxes forming a set',
    distractors: ['constitute', 'implement', 'accumulate'],
    senses: [
      {
        pos: 'verb',
        candidates: [
          { text: 'to be made up of the parts that are named', source: 'freedict', score: 0.9 },
          { text: 'be composed of', source: 'wordnet', score: 0.72 },
        ],
      },
    ],
    examples: [
      {
        text: 'The course will comprise six lectures and one long essay.',
        highlight: 'comprise',
        source: 'exam_corpus',
        score: 0.86,
      },
      {
        text: 'Older houses comprise about a third of the whole street.',
        highlight: 'comprise',
        source: 'exam_corpus',
        score: 0.78,
      },
    ],
  },

  /* ---------------- auxiliary words (promoted from OOV) ---------------- */
  {
    lemma: 'serene',
    role: 'auxiliary',
    phonetic: '/səˈriːn/',
    rank: 3204,
    etymology: 'From Latin "serenus" — clear, calm, bright.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'calm lake at dawn',
    distractors: ['jovial', 'lucid', 'ample'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'calm, quiet and free from worry', source: 'freedict', score: 0.92 },
          { text: 'not agitated; without loss of calm', source: 'wordnet', score: 0.75 },
        ],
      },
    ],
    examples: [
      {
        text: 'The garden was serene once the last visitors had gone.',
        highlight: 'serene',
        source: 'llm',
        score: 0.8,
      },
      {
        text: 'She kept a serene face through the whole long meeting.',
        highlight: 'serene',
        source: 'llm',
        score: 0.74,
      },
    ],
  },
  {
    lemma: 'kindly',
    role: 'auxiliary',
    phonetic: '/ˈkaɪndli/',
    rank: 3061,
    etymology: 'From Old English "gecyndelic" — natural, of a good kind.',
    etymology_source: 'morfessor',
    stage: 'ready',
    image_query: 'elderly person helping neighbour',
    distractors: ['courteous', 'sincere', 'benevolent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'warm and gentle toward other people', source: 'freedict', score: 0.9 },
          { text: 'pleasant and agreeable in manner', source: 'wordnet', score: 0.74 },
        ],
      },
      {
        pos: 'adv',
        candidates: [{ text: 'in a warm and gentle way', source: 'freedict', score: 0.82 }],
      },
    ],
    examples: [
      {
        text: 'He spoke kindly to every student who came to the desk.',
        highlight: 'kindly',
        source: 'llm',
        score: 0.79,
      },
      {
        text: 'A kindly word at the right moment can change a whole day.',
        highlight: 'kindly',
        source: 'llm',
        score: 0.73,
      },
    ],
  },
  {
    lemma: 'thorough',
    role: 'auxiliary',
    phonetic: '/ˈθʌrə/',
    rank: 2944,
    etymology: 'From Old English "þuruh" — through, complete.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'deep cleaning a room completely',
    distractors: ['meticulous', 'diligent', 'ample'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          {
            text: 'complete, with ample care and nothing left out',
            source: 'freedict',
            score: 0.91,
          },
          { text: 'performed comprehensively and completely', source: 'wordnet', score: 0.76 },
        ],
      },
    ],
    examples: [
      {
        text: 'A thorough check of the wiring took most of the morning.',
        highlight: 'thorough',
        source: 'llm',
        score: 0.78,
      },
      {
        text: 'The report is thorough and answers every early question.',
        highlight: 'thorough',
        source: 'llm',
        score: 0.72,
      },
    ],
  },
  {
    lemma: 'cautious',
    role: 'auxiliary',
    phonetic: '/ˈkɔːʃəs/',
    rank: 2833,
    etymology: 'From Latin "cautus" — on guard, from "cavere" (to beware).',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'person testing ice with foot',
    distractors: ['wary', 'prudent', 'reluctant'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'careful and wary about risk or harm', source: 'freedict', score: 0.92 },
          { text: 'showing careful forethought', source: 'wordnet', score: 0.75 },
        ],
      },
    ],
    examples: [
      {
        text: 'They gave a cautious welcome to the early results.',
        highlight: 'cautious',
        source: 'llm',
        score: 0.8,
      },
      {
        text: 'Be cautious on the stairs while the light is broken.',
        highlight: 'cautious',
        source: 'llm',
        score: 0.71,
      },
    ],
  },
  {
    lemma: 'sincere',
    role: 'auxiliary',
    phonetic: '/sɪnˈsɪə/',
    rank: 2712,
    etymology: 'From Latin "sincerus" — clean, pure, sound.',
    etymology_source: 'wiktionary',
    stage: 'pending_approval',
    image_query: 'honest handshake between two people',
    distractors: ['candid', 'courteous', 'kindly'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'honest and free from pretence', source: 'freedict', score: 0.91 },
          { text: 'open and genuine; not deceitful', source: 'wordnet', score: 0.77 },
        ],
      },
    ],
    examples: [
      {
        text: 'Her thanks were short but clearly sincere.',
        highlight: 'sincere',
        source: 'llm',
        score: 0.77,
      },
      {
        text: 'A sincere apology costs nothing and settles much.',
        highlight: 'sincere',
        source: 'llm',
        score: 0.7,
      },
    ],
  },
  {
    lemma: 'courteous',
    role: 'auxiliary',
    phonetic: '/ˈkɜːtiəs/',
    rank: 3122,
    etymology: 'From Old French "corteis" — having manners fit for a court.',
    etymology_source: 'wiktionary',
    stage: 'no_image',
    image_query: 'shop assistant greeting customer politely',
    distractors: ['kindly', 'sincere', 'jovial'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'polite and respectful toward other people', source: 'freedict', score: 0.9 },
          { text: 'characterized by good manners', source: 'wordnet', score: 0.74 },
        ],
      },
    ],
    examples: [
      {
        text: 'Even under pressure she remained courteous to everyone.',
        highlight: 'courteous',
        source: 'llm',
        score: 0.78,
      },
      {
        text: 'A courteous reply arrived within the hour.',
        highlight: 'courteous',
        source: 'llm',
        score: 0.69,
      },
    ],
  },
  {
    lemma: 'steadfast',
    role: 'auxiliary',
    phonetic: '/ˈstedfɑːst/',
    rank: 3350,
    etymology: 'From Old English "stedefæst" — fixed in place.',
    etymology_source: 'morfessor',
    stage: 'thin',
    image_query: 'lighthouse standing in storm',
    distractors: ['tenacious', 'resilient', 'diligent'],
    senses: [
      {
        pos: 'adj',
        candidates: [{ text: 'firm and not changing in purpose', source: 'freedict', score: 0.89 }],
      },
    ],
    examples: [
      {
        text: 'He was steadfast in his support through every setback.',
        highlight: 'steadfast',
        source: 'llm',
        score: 0.75,
      },
    ],
  },
  {
    lemma: 'ample',
    role: 'auxiliary',
    phonetic: '/ˈæmpl/',
    rank: 2666,
    etymology: 'From Latin "amplus" — large, spacious.',
    etymology_source: 'wiktionary',
    stage: 'ready',
    image_query: 'wide open storage room',
    distractors: ['frugal', 'mundane', 'thorough'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'more than enough in amount or size', source: 'freedict', score: 0.91 },
          { text: 'affording an abundant supply', source: 'wordnet', score: 0.75 },
        ],
      },
    ],
    examples: [
      {
        text: 'There is ample room for the whole group in this hall.',
        highlight: 'ample',
        source: 'llm',
        score: 0.79,
      },
      {
        text: 'Two weeks gives ample time to finish the reading.',
        highlight: 'ample',
        source: 'llm',
        score: 0.72,
      },
    ],
  },
  {
    lemma: 'fluent',
    role: 'auxiliary',
    phonetic: '/ˈfluːənt/',
    rank: 2977,
    etymology: 'From Latin "fluens" — flowing, from "fluere" (to flow).',
    etymology_source: 'wiktionary',
    stage: 'distractor_gap',
    image_query: 'person speaking easily on stage',
    distractors: ['eloquent', 'lucid', 'coherent'],
    senses: [
      {
        pos: 'adj',
        candidates: [
          { text: 'able to speak or write with easy flow', source: 'freedict', score: 0.92 },
          { text: 'expressing yourself smoothly and readily', source: 'wordnet', score: 0.76 },
        ],
      },
    ],
    examples: [
      {
        text: 'She is fluent in three languages and reads a fourth.',
        highlight: 'fluent',
        source: 'llm',
        score: 0.81,
      },
      {
        text: 'His writing became fluent after a year of daily practice.',
        highlight: 'fluent',
        source: 'llm',
        score: 0.73,
      },
    ],
  },

  /* ---------------- base words (assumed known, never built) ---------------- */
  { lemma: 'good', role: 'base', rank: 12, stage: 'base' },
  { lemma: 'make', role: 'base', rank: 21, stage: 'base' },
  { lemma: 'help', role: 'base', rank: 64, stage: 'base' },
  { lemma: 'small', role: 'base', rank: 88, stage: 'base' },
  { lemma: 'large', role: 'base', rank: 91, stage: 'base' },
  { lemma: 'clear', role: 'base', rank: 103, stage: 'base' },
  { lemma: 'learn', role: 'base', rank: 55, stage: 'base' },
  { lemma: 'careful', role: 'base', rank: 174, stage: 'base' },
];

/**
 * Lemmas that appear in selected definitions but match no word row. The
 * reconciler syncs these into `oos_queue`; the console is where a human either
 * promotes them to auxiliary or picks a rewrite.
 */
export const OOS_LEMMAS = ['altruistic', 'whim', 'aggravate', 'erode', 'unyielding'] as const;

/** LLM rewrite drafts already parked as `llm_rewrite` candidates or pending. */
export const REWRITE_DRAFTS: Record<string, string> = {
  altruistic: 'showing a kindly wish to help those in need',
  whim: 'chosen by personal wish rather than by reason or rule',
  aggravate: 'to make an already unpleasant state of affairs worse',
  erode: 'to wear away the base of something over a long time',
  unyielding: 'holding on firmly and refusing to change course',
};
