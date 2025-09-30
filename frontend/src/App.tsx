import { Router, Route } from '@solidjs/router'
import { lazy } from 'solid-js'
import Layout from './components/Layout'
import './styles/global.css'

const Home = lazy(() => import('./pages/Home'))
const CodeSystems = lazy(() => import('./pages/CodeSystems'))
const ValueSets = lazy(() => import('./pages/ValueSets'))
const ConceptMaps = lazy(() => import('./pages/ConceptMaps'))

export default function App() {
  return (
    <Router root={Layout}>
      <Route path="/" component={Home} />
      <Route path="/codesystems" component={CodeSystems} />
      <Route path="/valuesets" component={ValueSets} />
      <Route path="/conceptmaps" component={ConceptMaps} />
    </Router>
  )
}