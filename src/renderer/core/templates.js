/* Mindloom — starter templates */
(function (ML) {
  'use strict';

  const T = (id, name, icon, desc, outline, settings, decorate) => ({ id, name, icon, desc, outline, settings, decorate });

  const TEMPLATES = [
    T('blank', 'Blank Map', '◎', 'Start from a single central topic.', 'Central Topic', { theme: 'fresh', layout: 'mindmap' }),
    T('brainstorm', 'Brainstorm', '💡', 'Explore ideas around one question.',
      `Big Idea
  Why?
    Problem we solve
    Who benefits
  What?
    Core features
    Nice to have
  How?
    Resources
    Risks
  Wild ideas
    Moonshot
    What if…`, { theme: 'fresh', layout: 'mindmap', lineStyle: 'tapered' }),
    T('project', 'Project Plan', '🗂️', 'Phases, owners and milestones.',
      `Project Launch
  Goals
    Objective
    Success metrics
  Phase 1 · Discovery
    [x] Research
    [ ] Requirements
  Phase 2 · Build
    [ ] Design
    [ ] Development
    [ ] Testing
  Phase 3 · Launch
    [ ] Marketing
    [ ] Release
  Team
    Owner
    Stakeholders
  Risks`, { theme: 'business', layout: 'right', lineStyle: 'rounded', shapes: { root: 'rounded', main: 'rect', sub: 'underline' } },
      (doc) => { doc.root.children[1].icons = ['pr50']; doc.root.children[2].icons = ['pr0']; doc.root.children[5].icons = ['⚠️']; }),
    T('swot', 'SWOT Analysis', '🧭', 'Strengths, weaknesses, opportunities, threats.',
      `SWOT Analysis
  Strengths
    Brand
    Team
  Weaknesses
    Budget
    Reach
  Opportunities
    New markets
    Partnerships
  Threats
    Competitors
    Regulation`, { theme: 'classic', layout: 'mindmap', rainbow: true },
      (doc) => { const c = doc.root.children; c[0].icons = ['👍']; c[1].icons = ['👎']; c[2].icons = ['🚀']; c[3].icons = ['⚠️']; }),
    T('meeting', 'Meeting Notes', '📝', 'Agenda, decisions and action items.',
      `Weekly Meeting
  Attendees
  Agenda
    Updates
    Blockers
  Decisions
  Action Items
    [ ] Follow up
    [ ] Share notes
  Next meeting`, { theme: 'ocean', layout: 'tree', lineStyle: 'rounded', shapes: { root: 'rounded', main: 'none', sub: 'none' } }),
    T('book', 'Book Summary', '📚', 'Capture key ideas from a book.',
      `Book Title
  Author & context
  Key ideas
    Idea 1
    Idea 2
    Idea 3
  Favourite quotes
  Takeaways
    Apply at work
    Apply in life`, { theme: 'earth', layout: 'mindmap', shapes: { root: 'ellipse', main: 'pill', sub: 'underline' }, lineStyle: 'tapered' }),
    T('week', 'Weekly Planner', '📅', 'Plan the week day by day.',
      `This Week
  Monday
  Tuesday
  Wednesday
  Thursday
  Friday
  Weekend`, { theme: 'candy', layout: 'timeline', lineStyle: 'rounded', shapes: { root: 'pill', main: 'pill', sub: 'underline' } }),
    T('org', 'Org Chart', '🏢', 'Show a team structure.',
      `CEO
  CTO
    Engineering
    IT
  CFO
    Finance
    Legal
  CMO
    Marketing
    Sales`, { theme: 'business', layout: 'down', lineStyle: 'elbow', shapes: { root: 'rect', main: 'rect', sub: 'rect' } }),
    T('decision', 'Decision', '⚖️', 'Compare options with pros and cons.',
      `Decision
  Option A
    Pros
    Cons
  Option B
    Pros
    Cons
  Criteria
    Cost
    Time
    Impact`, { theme: 'lavender', layout: 'mindmap' }),
    T('fishbone', 'Root Cause', '🐟', 'Fishbone (Ishikawa) cause analysis.',
      `Problem
  People
    Training
  Process
    Approval delays
  Tools
    Outdated software
  Materials
  Environment
  Measurement`, { theme: 'sunset', layout: 'fishbone', lineStyle: 'straight', shapes: { root: 'rounded', main: 'rounded', sub: 'underline' } }),
    T('study', 'Study Notes', '🎓', 'Organise a topic for revision.',
      `Topic
  Definitions
  Key concepts
    Concept A
    Concept B
  Examples
  Questions
  Summary`, { theme: 'chalk', layout: 'mindmap', font: 'hand', shapes: { root: 'cloud', main: 'rounded', sub: 'underline' } }),
    T('goals', 'Goals & OKRs', '🎯', 'Objectives with key results.',
      `Goals 2026
  Health
    Run 10k
    Sleep 8h
  Career
    Learn a skill
    Ship side project
  Finance
    Save 20%
  Relationships`, { theme: 'midnight', layout: 'radial', lineStyle: 'curve', shapes: { root: 'circle', main: 'ellipse', sub: 'pill' } }),
  ];

  function build(id) {
    const tpl = TEMPLATES.find((t) => t.id === id) || TEMPLATES[0];
    const doc = ML.io.fromText(tpl.outline);
    Object.assign(doc.settings, ML.model.clone(tpl.settings || {}));
    doc.settings.shapes = Object.assign({}, ML.themes.DEFAULT_SETTINGS.shapes, (tpl.settings && tpl.settings.shapes) || {});
    if (tpl.decorate) tpl.decorate(doc);
    return doc;
  }

  ML.templates = { TEMPLATES, build };
})(typeof module !== 'undefined' && module.exports ? (global.ML = global.ML || {}) : (window.ML = window.ML || {}));
